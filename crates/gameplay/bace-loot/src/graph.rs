use bace_content::{LootGraphV1, LootSelectionV1};
use bace_random::{Domain, RandomError, RandomRoot, RandomStream};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct LootDrop {
    pub mutations: Vec<bace_gameplay_api::GeneratedItemMutation>,
    pub template: u32,
    pub stack: u32,
    pub node: String,
}
#[derive(Debug, PartialEq, Eq)]
pub enum GraphError {
    Invalid(String),
    Random(RandomError),
    Capacity,
}
impl From<RandomError> for GraphError {
    fn from(e: RandomError) -> Self {
        Self::Random(e)
    }
}
struct Task {
    item: Option<usize>,
    node: usize,
    stream: RandomStream,
}
/// Reusable bounded scratch. Caller output is unchanged on any failure.
pub struct LootScratch {
    tasks: Vec<Task>,
    pending: Vec<LootDrop>,
}
impl Default for LootScratch {
    fn default() -> Self {
        Self {
            tasks: Vec::with_capacity(4096),
            pending: Vec::with_capacity(256),
        }
    }
}
pub struct LootGraph {
    data: LootGraphV1,
    targets: Vec<Vec<usize>>,
    root: usize,
}
impl LootGraph {
    pub fn prepare(data: LootGraphV1) -> Result<Self, GraphError> {
        data.validate().map_err(GraphError::Invalid)?;
        let keys: BTreeMap<_, _> = data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let root = keys[data.root.as_str()];
        let targets = data
            .nodes
            .iter()
            .map(|n| n.branches.iter().map(|b| keys[b.target.as_str()]).collect())
            .collect();
        Ok(Self {
            data,
            targets,
            root,
        })
    }
    pub fn id(&self) -> u32 {
        self.data.id
    }
    pub fn generate(
        &self,
        root: &RandomRoot,
        event: [u8; 16],
        out: &mut Vec<LootDrop>,
        scratch: &mut LootScratch,
    ) -> Result<(), GraphError> {
        scratch.tasks.clear();
        scratch.pending.clear();
        scratch.tasks.push(Task {
            node: self.root,
            item: None,
            stream: root
                .event_stream(event, Domain::OrdinaryLoot)?
                .fork(b"table", u64::from(self.data.id))?,
        });
        let mut visited = 0;
        while let Some(task) = scratch.tasks.pop() {
            visited += 1;
            if visited > 4096 {
                return Err(GraphError::Capacity);
            }
            let node = &self.data.nodes[task.node];
            let mut stream = task.stream.fork(node.id.as_bytes(), 0)?;
            match node.selection {
                LootSelectionV1::Nothing => {}
                LootSelectionV1::Item => {
                    if scratch.pending.len() == 256 {
                        return Err(GraphError::Capacity);
                    }
                    let item = node.item.as_ref().expect("validated item");
                    let stack = item.minimum_stack
                        + stream.below(u64::from(item.maximum_stack - item.minimum_stack) + 1)?
                            as u32;
                    scratch.pending.push(LootDrop {
                        template: item.template,
                        stack,
                        node: node.id.clone(),
                        mutations: Vec::new(),
                    });
                    let item = Some(scratch.pending.len() - 1);
                    for index in (0..node.branches.len()).rev() {
                        let branch = &node.branches[index];
                        self.push_branch(
                            task.node,
                            index,
                            &stream.fork(branch.id.as_bytes(), 0)?,
                            item,
                            scratch,
                        )?;
                    }
                }
                LootSelectionV1::Mutation => {
                    let item = task
                        .item
                        .ok_or_else(|| GraphError::Invalid("mutation without item".into()))?;
                    let value = crate::mutation::sample(
                        node.mutation.as_ref().expect("validated mutation"),
                        &mut stream,
                    )?;
                    let mutations = &mut scratch.pending[item].mutations;
                    if mutations.len() >= 128 {
                        return Err(GraphError::Capacity);
                    }
                    if mutations
                        .iter()
                        .any(|old| crate::mutation::key(old) == crate::mutation::key(&value))
                    {
                        return Err(GraphError::Invalid("duplicate property mutation".into()));
                    }
                    mutations.push(value);
                }
                LootSelectionV1::All | LootSelectionV1::Independent => {
                    for index in (0..node.branches.len()).rev() {
                        let branch = &node.branches[index];
                        let mut branch_stream = stream.fork(branch.id.as_bytes(), 0)?;
                        let selected = match branch.probability {
                            None => true,
                            Some(p) => branch_stream.chance(p.numerator, p.denominator)?,
                        };
                        if selected {
                            self.push_branch(task.node, index, &branch_stream, task.item, scratch)?;
                        }
                    }
                }
                LootSelectionV1::Weighted => {
                    let total: u64 = node.branches.iter().map(|b| b.weight.unwrap_or(0)).sum();
                    let draw = stream.below(total)?;
                    let mut end = 0;
                    for (index, branch) in node.branches.iter().enumerate() {
                        end += branch.weight.unwrap_or(0);
                        if draw < end {
                            self.push_branch(
                                task.node,
                                index,
                                &stream.fork(branch.id.as_bytes(), 0)?,
                                task.item,
                                scratch,
                            )?;
                            break;
                        }
                    }
                }
                LootSelectionV1::OrderedCumulative => {
                    let draw = stream.below(
                        node.branches[0]
                            .probability
                            .expect("validated probability")
                            .denominator,
                    )?;
                    let mut end = 0;
                    for (index, branch) in node.branches.iter().enumerate() {
                        end += branch.probability.expect("validated probability").numerator;
                        if draw < end {
                            self.push_branch(
                                task.node,
                                index,
                                &stream.fork(branch.id.as_bytes(), 0)?,
                                task.item,
                                scratch,
                            )?;
                            break;
                        }
                    }
                }
            }
        }
        if scratch.pending.len() > out.capacity() - out.len() {
            return Err(GraphError::Capacity);
        }
        out.append(&mut scratch.pending);
        Ok(())
    }
    fn push_branch(
        &self,
        parent: usize,
        index: usize,
        stream: &RandomStream,
        item: Option<usize>,
        scratch: &mut LootScratch,
    ) -> Result<(), GraphError> {
        let branch = &self.data.nodes[parent].branches[index];
        let count = branch.minimum_rolls
            + stream
                .fork(b"count", 0)?
                .below(u64::from(branch.maximum_rolls - branch.minimum_rolls) + 1)?
                as u32;
        if scratch.tasks.len() + count as usize > 4096 {
            return Err(GraphError::Capacity);
        }
        for occurrence in (0..count).rev() {
            scratch.tasks.push(Task {
                node: self.targets[parent][index],
                item,
                stream: stream.fork(b"repeat", u64::from(occurrence))?,
            });
        }
        Ok(())
    }
}
