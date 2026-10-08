use super::*;
impl VisibilityService {
    pub fn accept_visibility(
        &mut self,
        outcome: VisibilityOutcome,
    ) -> Result<(), VisibilityServiceError> {
        let Some((&key, _)) = self
            .observers
            .iter()
            .find(|(_, o)| o.query == Some(outcome.correlation))
        else {
            return Ok(());
        };
        let o = self.observers.get_mut(&key).expect("found");
        o.query = None;
        let mut snapshot = match outcome.result {
            Ok(s) => s,
            Err((_, buffer)) => {
                o.buffer = buffer;
                return Ok(());
            }
        };
        // A missing cold model is never replaced with a placeholder or recorded
        // as client knowledge. Hidden source objects use the normal PVS grace.
        snapshot.candidates.retain(|c| {
            self.objects.get(&c.entity).is_some_and(|object| {
                object.blueprint.admitted_tick <= snapshot.tick
                    && object.blueprint.description.game.description_flags & 0x80 == 0
            })
        });
        let ids: Vec<_> = snapshot.candidates.iter().map(|c| c.entity).collect();
        if let Err((_, snapshot)) = o.knowledge.stage(snapshot) {
            o.buffer = snapshot.candidates;
            return Err(VisibilityServiceError::Projection);
        }
        for id in &ids {
            if o.sent
                .get(id)
                .zip(self.objects.get(id))
                .is_some_and(|(sent, current)| {
                    sent.blueprint.incarnation != current.blueprint.incarnation
                })
            {
                o.knowledge
                    .recreate_pending(*id)
                    .map_err(|_| VisibilityServiceError::Projection)?;
            }
        }
        let delta = o.knowledge.pending().expect("staged");
        let mut entities: Vec<_> = ids
            .into_iter()
            .filter(|id| o.knowledge.knows(*id) || delta.creates.binary_search(id).is_ok())
            .collect();
        if self.objects.contains_key(&o.binding.actor) {
            entities.push(o.binding.actor);
            entities.sort_unstable();
        }
        if entities.len() > 4097 {
            return Err(VisibilityServiceError::Capacity);
        }
        let correlation = self.next()?;
        let o = self.observers.get_mut(&key).expect("found");
        o.view_request = Some(ObjectViewRequest {
            correlation,
            binding: o.binding,
            entities,
        });
        Ok(())
    }
    pub fn reliable_admission(
        &mut self,
        key: SessionKey,
        correlation: u64,
        accepted: bool,
    ) -> Result<bool, VisibilityServiceError> {
        let Some(o) = self.observers.get_mut(&key) else {
            return Ok(false);
        };
        let Some(p) = o
            .publication
            .as_mut()
            .filter(|p| p.inflight == Some(correlation))
        else {
            return Ok(false);
        };
        if !accepted {
            o.failed = Some(VisibilityServiceError::Rejected);
            return Err(VisibilityServiceError::Rejected);
        }
        p.inflight = None;
        p.messages.pop_front();
        if p.messages.is_empty() {
            let p = o.publication.take().expect("present");
            self.retained_bytes -= p.bytes;
            if let Some(snapshot) = o
                .knowledge
                .commit(p.ticket)
                .map_err(|_| VisibilityServiceError::Projection)?
            {
                o.buffer = snapshot.candidates;
            }
            for id in p.removes {
                o.sent.remove(&id);
            }
            o.sent.extend(p.sent);
            if let Some(sent) = p.self_sent {
                o.self_sent = Some(sent);
            }
        }
        Ok(true)
    }
    pub fn drive(
        &mut self,
        input: &SimulationInput,
        network: &NetworkThread,
    ) -> Result<(), VisibilityServiceError> {
        self.drive_mode(input, network, false)
    }
    pub(super) fn drive_pending(
        &mut self,
        input: &SimulationInput,
        network: &NetworkThread,
    ) -> Result<(), VisibilityServiceError> {
        self.drive_mode(input, network, true)
    }
    fn drive_mode(
        &mut self,
        input: &SimulationInput,
        network: &NetworkThread,
        pending_only: bool,
    ) -> Result<(), VisibilityServiceError> {
        // Round-robin one observer per poll keeps peer count from starving other
        // lifecycle/durability services; results are drained separately.
        let key = self
            .observers
            .keys()
            .find(|key| self.cursor.is_none_or(|old| **key > old))
            .copied()
            .or_else(|| self.observers.keys().next().copied());
        let Some(key) = key else {
            return Ok(());
        };
        self.cursor = Some(key);
        if self.observers[&key].failed.is_some() {
            return Ok(());
        }
        let o = self.observers.get_mut(&key).expect("selected");
        if let Some(p) = &o.publication {
            if p.inflight.is_some() {
                return Ok(());
            }
            let correlation = self.next()?;
            let o = self.observers.get_mut(&key).expect("selected");
            let p = o.publication.as_mut().expect("present");
            let command = NetworkCommand::SendReliableBatch {
                key,
                correlation,
                messages: p.messages.front().expect("nonempty").clone(),
            };
            match network.try_send(command) {
                Ok(()) => p.inflight = Some(correlation),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err(VisibilityServiceError::NetworkClosed);
                }
            }
            return Ok(());
        }
        if o.query.is_some() || o.view_query.is_some() {
            return Ok(());
        }
        if let Some(request) = o.view_request.take() {
            let correlation = request.correlation;
            match input.try_submit(Command::ObjectView(Box::new(request))) {
                Ok(()) => o.view_query = Some(correlation),
                Err(TrySendError::Full(Command::ObjectView(request))) => {
                    o.view_request = Some(*request)
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err(VisibilityServiceError::WorkerClosed);
                }
                _ => return Err(VisibilityServiceError::Projection),
            }
            return Ok(());
        }
        if let Some(tick) = o.reset_requested {
            return match self.reset_observer(key, tick) {
                Err(VisibilityServiceError::Busy) => Ok(()),
                result => result,
            };
        }
        if !o.retirements.is_empty() {
            return self.publish_retirement(key);
        }
        if pending_only || !o.launches.is_empty() {
            return Ok(());
        }
        let correlation = self.next()?;
        let o = self.observers.get_mut(&key).expect("selected");
        let request = VisibilityRequest {
            correlation,
            binding: o.binding,
            limit: self.limits.objects,
            candidates: std::mem::take(&mut o.buffer),
        };
        match input.try_submit(Command::Visibility(Box::new(request))) {
            Ok(()) => o.query = Some(correlation),
            Err(TrySendError::Full(Command::Visibility(request))) => o.buffer = request.candidates,
            Err(TrySendError::Disconnected(_)) => return Err(VisibilityServiceError::WorkerClosed),
            _ => return Err(VisibilityServiceError::Projection),
        }
        Ok(())
    }
    pub(super) fn publish_retirement(
        &mut self,
        key: SessionKey,
    ) -> Result<(), VisibilityServiceError> {
        let o = self
            .observers
            .get_mut(&key)
            .ok_or(VisibilityServiceError::Stale)?;
        let (id, tick) = o
            .retirements
            .front()
            .copied()
            .ok_or(VisibilityServiceError::Stale)?;
        let delta = o
            .knowledge
            .stage_retirement(id, tick)
            .map_err(|_| VisibilityServiceError::Stale)?
            .clone();
        let mut messages = Vec::new();
        if let Some(sent) = o.sent.get(&id) {
            append_deletes(&mut messages, &sent.blueprint);
        }
        match self.publish(key, delta.ticket, messages, BTreeMap::new(), delta.removes) {
            Ok(()) => {
                self.observers
                    .get_mut(&key)
                    .expect("retained observer")
                    .retirements
                    .pop_front();
                Ok(())
            }
            Err(error) => {
                self.observers
                    .get_mut(&key)
                    .expect("retained observer")
                    .knowledge
                    .discard_unpublished(delta.ticket)
                    .map_err(|_| VisibilityServiceError::Projection)?;
                if error == VisibilityServiceError::Capacity {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }
    pub(super) fn publish(
        &mut self,
        key: SessionKey,
        ticket: u64,
        messages: Vec<ReplicationMessage>,
        sent: BTreeMap<EntityId, Sent>,
        removes: Vec<EntityId>,
    ) -> Result<(), VisibilityServiceError> {
        self.publish_with_self(key, ticket, messages, sent, removes, None)
    }
    pub(super) fn publish_with_self(
        &mut self,
        key: SessionKey,
        ticket: u64,
        messages: Vec<ReplicationMessage>,
        sent: BTreeMap<EntityId, Sent>,
        removes: Vec<EntityId>,
        self_sent: Option<Sent>,
    ) -> Result<(), VisibilityServiceError> {
        let bytes: usize = messages.iter().map(|m| m.bytes.len()).sum();
        if bytes > self.limits.retained_bytes - self.retained_bytes {
            return Err(VisibilityServiceError::Capacity);
        }
        let mut chunks = VecDeque::new();
        let mut chunk = Vec::new();
        let mut size = 0;
        for m in messages {
            if m.bytes.len() > self.limits.batch_bytes {
                return Err(VisibilityServiceError::Capacity);
            }
            if chunk.len() == 256 || size + m.bytes.len() > self.limits.batch_bytes {
                chunks.push_back(std::mem::take(&mut chunk));
                size = 0;
            }
            size += m.bytes.len();
            chunk.push((m.queue, m.bytes));
        }
        if !chunk.is_empty() {
            chunks.push_back(chunk);
        }
        let o = self.observers.get_mut(&key).expect("selected");
        if chunks.is_empty() {
            if let Some(snapshot) = o
                .knowledge
                .commit(ticket)
                .map_err(|_| VisibilityServiceError::Projection)?
            {
                o.buffer = snapshot.candidates;
            }
            for id in removes {
                o.sent.remove(&id);
            }
            o.sent.extend(sent);
            if let Some(sent) = self_sent {
                o.self_sent = Some(sent);
            }
            return Ok(());
        }
        self.retained_bytes += bytes;
        o.publication = Some(Publication {
            self_sent,
            ticket,
            messages: chunks,
            inflight: None,
            sent,
            removes,
            bytes,
        });
        Ok(())
    }
}
pub(super) fn append_deletes(out: &mut Vec<ReplicationMessage>, blueprint: &Blueprint) {
    for description in std::iter::once(&blueprint.description).chain(&blueprint.children) {
        out.push(ReplicationMessage {
            queue: 10,
            bytes: ObjectControl::Delete {
                object_id: description.object_id,
                instance_sequence: description.physics.sequences.instance,
            }
            .encode(),
        });
    }
}
