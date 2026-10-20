use super::fuzzy::{rank, FuzzyHit, Ranked};
use super::mock::{Conversation, MockWorld, Network};
use super::sidebar::ordered_conversations;

pub const JUMP_RESULT_LIMIT: usize = 20;

/// The label is the name, and a hit there outranks a hit on the detail (the
/// topic or real name), as Qt's Jump tiers them.
const NAME_WEIGHT: u32 = 2;
const DETAIL_WEIGHT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpTarget {
    pub kind: JumpKind,
    pub name: String,
    pub network_id: String,
    pub conversation_id: String,
    pub label: String,
    /// Byte offsets into `label` of the characters the query matched, drawn bold.
    pub label_indices: Vec<usize>,
    /// Whether the label itself matched. Detail-only hits sort below these.
    pub name_hit: bool,
    /// The best weighted field score.
    pub score: u32,
    pub order: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpKind {
    Conversation,
    Status,
}

/// One row before ranking, in sidebar order.
struct Candidate {
    kind: JumpKind,
    name: String,
    network_id: String,
    conversation_id: String,
    label: String,
    detail: String,
    order: usize,
}

pub fn jump_targets(world: &MockWorld, query: &str) -> Vec<JumpTarget> {
    let mut duplicate_names = std::collections::HashMap::<String, usize>::new();
    for network in &world.networks {
        for conv in &network.conversations {
            *duplicate_names
                .entry(conv.name.clone())
                .or_insert(0) += 1;
        }
    }

    let mut candidates = Vec::new();
    for network in &world.networks {
        let network_name = &network.display_name;
        // Sidebar order, so the candidate list follows the sidebar and includes collapsed networks.
        for conv in ordered_conversations(network) {
            candidates.push(Candidate {
                kind: JumpKind::Conversation,
                name: conv.name.clone(),
                network_id: network.id.clone(),
                conversation_id: format!("{}\n{}", network.id, conv.key()),
                label: conversation_label(&conv.name, network_name, &duplicate_names),
                detail: conversation_detail(conv, network),
                order: candidates.len(),
            });
        }
        candidates.push(Candidate {
            kind: JumpKind::Status,
            name: "Status".to_string(),
            network_id: network.id.clone(),
            conversation_id: String::new(),
            label: status_label(network_name),
            detail: String::new(),
            order: candidates.len(),
        });
    }

    let mut targets: Vec<JumpTarget> = rank(query, candidates, |c: &Candidate| {
        [
            (c.label.as_str(), NAME_WEIGHT),
            (c.detail.as_str(), DETAIL_WEIGHT),
        ]
    })
    .into_iter()
    .map(|ranked: Ranked<Candidate, 2>| {
        let [label_hit, _detail_hit] = ranked.fields;
        jump_target(ranked.item, ranked.score, label_hit)
    })
    .collect();

    // Name hits first. The sort is stable, so each tier keeps its score order, then sidebar order.
    targets.sort_by_key(|target| !target.name_hit);
    targets.truncate(JUMP_RESULT_LIMIT);
    targets
}

fn jump_target(candidate: Candidate, score: u32, label_hit: Option<FuzzyHit>) -> JumpTarget {
    JumpTarget {
        kind: candidate.kind,
        name: candidate.name,
        network_id: candidate.network_id,
        conversation_id: candidate.conversation_id,
        label: candidate.label,
        name_hit: label_hit.is_some(),
        label_indices: label_hit.map(|hit| hit.indices).unwrap_or_default(),
        score,
        order: candidate.order,
    }
}

fn conversation_label(
    name: &str,
    network_name: &str,
    duplicates: &std::collections::HashMap<String, usize>,
) -> String {
    if duplicates.get(name).copied().unwrap_or(0) > 1 && !network_name.is_empty() {
        format!("{name} · {network_name}")
    } else {
        name.to_string()
    }
}

fn status_label(network_name: &str) -> String {
    if network_name.is_empty() {
        "Status".to_string()
    } else {
        format!("{network_name} Status")
    }
}

fn conversation_detail(conv: &Conversation, _network: &Network) -> String {
    if conv.channel {
        conv.topic.clone().unwrap_or_default()
    } else {
        conv.peer_realname.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock;

    #[test]
    fn duplicate_omarchy_gets_network_suffix() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "omarchy");
        let labels: Vec<_> = targets
            .iter()
            .filter(|t| t.name == "#omarchy")
            .map(|t| t.label.clone())
            .collect();
        assert!(labels.iter().any(|l| l.contains('·')));
    }

    #[test]
    fn unique_names_no_suffix() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "desktop");
        assert!(targets.iter().any(|t| t.label == "#desktop"));
    }

    #[test]
    fn detail_only_ranks_below_name_hits() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "build");
        let name_idx = targets
            .iter()
            .position(|t| t.name_hit && t.name == "#build")
            .expect("#build name hit");
        let detail_idx = targets
            .iter()
            .position(|t| !t.name_hit && t.name == "#lab")
            .expect("#lab topic-only hit");
        assert!(name_idx < detail_idx);
    }

    #[test]
    fn fuzzy_subsequence_finds_omarchy() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "omy");
        let first = targets.first().expect("omy hits #omarchy");
        assert_eq!(first.name, "#omarchy");
        assert_eq!(first.network_id, "omarchy");
        // The label is "#omarchy · irc.example · fred": o, m and y, with gaps.
        assert_eq!(first.label_indices.len(), 3);
        let picked: Vec<char> = first
            .label_indices
            .iter()
            .map(|&at| first.label[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['o', 'm', 'y']);
    }

    #[test]
    fn label_indices_are_empty_for_detail_only_hits() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "build");
        let detail_only = targets
            .iter()
            .find(|t| !t.name_hit && t.name == "#lab")
            .expect("#lab topic-only hit");
        assert!(detail_only.label_indices.is_empty());
    }

    #[test]
    fn blank_query_keeps_sidebar_order_and_marks_no_letters() {
        let world = mock::load_world();
        let blank = jump_targets(&world, "");
        assert!(blank.iter().all(|t| t.label_indices.is_empty()));
        let orders: Vec<usize> = blank.iter().map(|t| t.order).collect();
        assert!(orders.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn jump_omarchy_picks_the_omarchy_network_first() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "#omarchy");
        let first = targets.first().expect("a #omarchy hit");
        assert_eq!(first.name, "#omarchy");
        assert_eq!(first.network_id, "omarchy");
        assert_eq!(first.label, "#omarchy · irc.example · fred");
    }

    #[test]
    fn jump_candidates_follow_sidebar_order_after_a_network_move() {
        let mut world = mock::load_world();
        crate::model::sidebar::move_network(&mut world, "omarchy", 1);
        let targets = jump_targets(&world, "");
        assert_eq!(targets.first().map(|t| t.network_id.as_str()), Some("oftc"));
        // Jump takes no collapse state, so a collapsed network still lists its rows.
        assert!(jump_targets(&world, "#build").iter().any(|t| t.network_id == "oftc"));
    }

    #[test]
    fn limit_twenty() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "");
        assert!(targets.len() <= JUMP_RESULT_LIMIT);
    }
}
