use crate::*;
use std::sync::Arc;

/// Concise representation of data held by various Agents on a single Node,
/// without having to refer to explicit op hashes or locations.
///
/// This type is a simplification of `ScenarioDef`, making the definition of
/// single-node scenarios even more concise. Rather than referring to op
/// locations directly, this type refers to ops (and their locations) by *index*.
/// The ops are generated by [`mock_agent_persistence`] and guaranteed to be
/// in ascending order by DHT location. Therefore this type is agnostic of the
/// actual location of anything, and only cares about "arc topology".
///
/// It's expected that we'll eventually have a small library of these scenarios,
/// defined in terms of this type.
///
/// See [`mock_agent_persistence`] for usage detail.
///
/// A bit of a historical note: this type was created before `ScenarioDef`, and
/// if it were the other way around, perhaps this type wouldn't exist. But it's
/// still nice how concise it is.
/// TODO: can this somehow be unified with `ScenarioDef`?
pub struct LocalScenarioDef {
    /// Total number of op hashes to be generated
    pub total_ops: usize,
    /// Declares arcs and ownership in terms of indices into a vec of generated op hashes.
    pub agents: Vec<LocalScenarioDefAgent>,
}

impl LocalScenarioDef {
    /// Construct this type from a compact "untagged" format using
    /// tuples instead of structs. This is intended to be the canonical constructor.
    pub fn from_compact(total_ops: usize, v: Vec<LocalScenarioDefAgentCompact>) -> Self {
        Self {
            total_ops,
            agents: v
                .into_iter()
                .map(|(agent, arc_indices, hash_indices)| LocalScenarioDefAgent {
                    agent,
                    arc_indices,
                    hash_indices,
                })
                .collect(),
        }
    }
}

/// Declares arcs and ownership in terms of indices into a vec of generated op hashes.
pub struct LocalScenarioDefAgent {
    /// The agent in question
    pub agent: Arc<KitsuneAgent>,
    /// The start and end op indices of the arc for this agent
    pub arc_indices: (usize, usize),
    /// The indices of ops to consider as owned
    pub hash_indices: Vec<usize>,
}

/// Same as [`LocalScenarioDefAgent`], but using a tuple instead of a struct.
/// It's just more compact.
pub type LocalScenarioDefAgentCompact = (Arc<KitsuneAgent>, (usize, usize), Vec<usize>);
