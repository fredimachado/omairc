use super::mock::{Conversation, MockWorld, Network};

pub const JUMP_RESULT_LIMIT: usize = 20;

pub fn jump_score(query: &str, name: &str, detail: &str) -> i32 {
    let folded = query.trim().to_lowercase();
    if folded.is_empty() {
        return 0;
    }
    let mut score = 0;
    if name.to_lowercase().contains(&folded) {
        score += 2;
    }
    if !detail.is_empty() && detail.to_lowercase().contains(&folded) {
        score += 1;
    }
    score
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpTarget {
    pub kind: JumpKind,
    pub name: String,
    pub network_id: String,
    pub conversation_id: String,
    pub label: String,
    pub score: i32,
    pub order: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpKind {
    Conversation,
    Status,
}

pub fn jump_targets(world: &MockWorld, query: &str) -> Vec<JumpTarget> {
    let folded = query.trim();
    let mut duplicate_names = std::collections::HashMap::<String, usize>::new();
    for network in &world.networks {
        for conv in &network.conversations {
            *duplicate_names
                .entry(conv.name.clone())
                .or_insert(0) += 1;
        }
    }

    let mut candidates = Vec::new();
    let mut order = 0usize;
    for network in &world.networks {
        let network_name = &network.display_name;
        for conv in &network.conversations {
            let label = conversation_label(&conv.name, network_name, &duplicate_names);
            let detail = conversation_detail(conv, network);
            let score = jump_score(folded, &label, &detail);
            if !folded.is_empty() && score <= 0 {
                continue;
            }
            candidates.push(JumpTarget {
                kind: JumpKind::Conversation,
                name: conv.name.clone(),
                network_id: network.id.clone(),
                conversation_id: format!("{}\n{}", network.id, conv.key()),
                label,
                score,
                order,
            });
            order += 1;
        }
        let status_label = status_label(network_name);
        let status_score = jump_score(folded, &status_label, "");
        if folded.is_empty() || status_score > 0 {
            candidates.push(JumpTarget {
                kind: JumpKind::Status,
                name: "Status".to_string(),
                network_id: network.id.clone(),
                conversation_id: String::new(),
                label: status_label,
                score: status_score,
                order,
            });
            order += 1;
        }
    }

    candidates.sort_by(|left, right| {
        let left_name = i32::from(left.score >= 2);
        let right_name = i32::from(right.score >= 2);
        right_name
            .cmp(&left_name)
            .then(left.order.cmp(&right.order))
    });

    if candidates.len() > JUMP_RESULT_LIMIT {
        candidates.truncate(JUMP_RESULT_LIMIT);
    }
    candidates
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
            .position(|t| t.score >= 2 && t.name == "#build")
            .expect("#build name hit");
        let detail_idx = targets
            .iter()
            .position(|t| t.score == 1 && t.name == "#lab")
            .expect("#lab topic-only hit");
        assert!(name_idx < detail_idx);
    }

    #[test]
    fn limit_twenty() {
        let world = mock::load_world();
        let targets = jump_targets(&world, "");
        assert!(targets.len() <= JUMP_RESULT_LIMIT);
    }
}
