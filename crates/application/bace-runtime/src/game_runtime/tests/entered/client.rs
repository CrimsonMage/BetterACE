//! Minimal transport peer for real UDP lifecycle inputs and reliable output.
use super::*;
use bace_wire::*;
use std::net::UdpSocket;
pub(in crate::game_runtime) struct Client {
    login: UdpSocket,
    receiver: UdpSocket,
    keys: Option<ClientKeys>,
    sender: Option<Isaac>,
    key: Option<SessionKey>,
    packet_sequence: u32,
    message_sequence: u32,
    received: std::collections::BTreeSet<u32>,
    highest: u32,
    reassembly: bace_transport::OrderedMessages,
    pub messages: Vec<bace_transport::ReceivedMessage>,
}
impl Client {
    pub fn new(runtime: &GameRuntime, account: &str) -> Self {
        let login = UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        login.set_nonblocking(true).unwrap();
        receiver.set_nonblocking(true).unwrap();
        // Independently compiled official inbound-login oracle payload.
        let hex = "0400313830320000E703000002000000010000003930000007004163636F756E7400000000000000130000001273796E7468657469632D70617373776F72640000";
        let mut payload: Vec<_> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(
            account.len(),
            7,
            "fixture login preserves oracle field widths"
        );
        let original = b"Account";
        let offset = payload
            .windows(original.len())
            .position(|v| v == original)
            .unwrap();
        payload[offset..offset + original.len()].copy_from_slice(account.as_bytes());
        let packet = encode_server_packet(
            PacketHeader {
                flags: flags::LOGIN_REQUEST,
                ..Default::default()
            },
            &payload,
            &[],
            0,
        )
        .unwrap();
        login
            .send_to(&packet, runtime.network.client_address)
            .unwrap();
        Self {
            login,
            receiver,
            keys: None,
            sender: None,
            key: None,
            packet_sequence: 2,
            message_sequence: 1,
            received: Default::default(),
            highest: 1,
            reassembly: bace_transport::OrderedMessages::new(Default::default(), 1024),
            messages: Vec::new(),
        }
    }
    pub fn bind(&mut self, key: SessionKey) {
        self.key = Some(key);
    }
    pub fn send_message(&mut self, runtime: &GameRuntime, key: SessionKey, bytes: &[u8]) {
        let fragments = bace_transport::fragment_message(
            self.message_sequence,
            9,
            bytes,
            runtime.limits.message_bytes,
        )
        .unwrap();
        self.message_sequence += 1;
        for fragment in fragments {
            let packet = encode_server_packet(
                PacketHeader {
                    sequence: self.packet_sequence,
                    flags: flags::ENCRYPTED_CHECKSUM | flags::BLOB_FRAGMENTS,
                    id: key.id,
                    iteration: 1,
                    ..Default::default()
                },
                &[],
                &[fragment],
                self.sender
                    .as_mut()
                    .expect("connected client seed")
                    .next_key(),
            )
            .unwrap();
            self.packet_sequence += 1;
            self.login
                .send_to(&packet, runtime.network.client_address)
                .unwrap();
        }
    }
    pub fn has_create(&self, object: u32, start: usize) -> bool {
        self.messages.iter().skip(start).any(|message| {
            message.queue == 10
                && message
                    .bytes
                    .starts_with(&opcode::GameMessageOpcode::ObjectCreate.0.to_le_bytes())
                && message.bytes.get(4..8) == Some(object.to_le_bytes().as_slice())
        })
    }
    pub fn has_delete(&self, object: u32, start: usize) -> bool {
        self.messages.iter().skip(start).any(|message| {
            message.queue == 10
                && message
                    .bytes
                    .starts_with(&opcode::GameMessageOpcode::ObjectDelete.0.to_le_bytes())
                && message.bytes.get(4..8) == Some(object.to_le_bytes().as_slice())
        })
    }
    pub fn assert_entry_order(&self, actor: u32, start: usize) {
        let messages = &self.messages[start..];
        let event_index = |kind: u32| {
            messages.iter().position(|message| {
                message.queue == 9
                    && message
                        .bytes
                        .starts_with(&opcode::GameMessageOpcode::GameEvent.0.to_le_bytes())
                    && message.bytes.get(4..8) == Some(actor.to_le_bytes().as_slice())
                    && message.bytes.get(12..16) == Some(kind.to_le_bytes().as_slice())
            })
        };
        let description = event_index(opcode::GameEventType::PlayerDescription.0)
            .expect("entry player description reached UDP peer");
        let titles = event_index(opcode::GameEventType::CharacterTitle.0)
            .expect("entry titles reached UDP peer");
        let friends = event_index(opcode::GameEventType::FriendsListUpdate.0)
            .expect("entry friends reached UDP peer");
        assert!(
            description < titles && titles < friends,
            "ordered queue-9 entry events"
        );
        let player_create = messages
            .iter()
            .position(|message| {
                message.queue == 10
                    && message
                        .bytes
                        .starts_with(&opcode::GameMessageOpcode::PlayerCreate.0.to_le_bytes())
                    && message.bytes.get(4..8) == Some(actor.to_le_bytes().as_slice())
            })
            .expect("entry PlayerCreate reached UDP peer");
        let self_create = messages
            .iter()
            .position(|message| {
                message.queue == 10
                    && message
                        .bytes
                        .starts_with(&opcode::GameMessageOpcode::ObjectCreate.0.to_le_bytes())
                    && message.bytes.get(4..8) == Some(actor.to_le_bytes().as_slice())
            })
            .expect("entry self ObjectCreate reached UDP peer");
        assert!(player_create < self_create, "ordered queue-10 self create");
    }
    pub fn drain(&mut self, runtime: &GameRuntime) {
        let mut buffer = [0; 1025];
        let mut packets = Vec::new();
        for socket in 0..2 {
            for _ in 0..256 {
                let received = if socket == 0 {
                    self.login.recv_from(&mut buffer)
                } else {
                    self.receiver.recv_from(&mut buffer)
                };
                let Ok((size, _)) = received else {
                    break;
                };
                let packet = Datagram::decode(&buffer[..size]).unwrap();
                if packet.header.flags == flags::CONNECT_REQUEST {
                    let challenge = ConnectRequest::decode(packet.body).unwrap();
                    self.keys = Some(ClientKeys::new(challenge.server_seed));
                    self.sender = Some(Isaac::new(challenge.client_seed));
                    let connect = encode_server_packet(
                        PacketHeader {
                            flags: flags::CONNECT_RESPONSE,
                            ..Default::default()
                        },
                        &challenge.cookie.to_le_bytes(),
                        &[],
                        0,
                    )
                    .unwrap();
                    self.receiver
                        .send_to(&connect, runtime.network.server_address)
                        .unwrap();
                } else {
                    packets.push(buffer[..size].to_vec());
                }
            }
        }
        let mut changed = false;
        for packet_bytes in packets {
            let header = Datagram::decode(&packet_bytes).unwrap().header;
            if header.sequence > 1
                && (header.sequence <= self.highest || !self.received.insert(header.sequence))
            {
                continue;
            }
            assert!(self.received.len() <= 256, "bounded receive window");
            let packet = bace_transport::decode_transport_packet(
                &packet_bytes,
                self.keys.as_mut().expect("connected peer keys"),
            )
            .unwrap();
            while self.received.remove(&(self.highest + 1)) {
                self.highest += 1;
            }
            changed = true;
            for fragment in packet.fragments {
                for message in self
                    .reassembly
                    .insert(
                        fragment,
                        runtime.clock.monotonic.elapsed().as_millis() as u64,
                    )
                    .unwrap()
                {
                    assert!(self.messages.len() < 16384, "bounded test output capture");
                    self.messages.push(message);
                }
            }
        }
        if changed && let Some(key) = self.key {
            let missing: Vec<_> = self
                .received
                .first()
                .map(|next| ((self.highest + 1)..*next).take(16).collect())
                .unwrap_or_default();
            let (bits, optional) = OptionalHeaders {
                ack: Some(self.highest),
                retransmit: (!missing.is_empty()).then_some(missing),
                ..Default::default()
            }
            .encode()
            .unwrap();
            let ack = encode_server_packet(
                PacketHeader {
                    flags: bits,
                    id: key.id,
                    iteration: 1,
                    ..Default::default()
                },
                &optional,
                &[],
                0,
            )
            .unwrap();
            self.login
                .send_to(&ack, runtime.network.client_address)
                .unwrap();
        }
    }
}
