use super::fuzzy::{rank, FuzzyHit, Ranked};
use crate::controller::controller::Controller;
use crate::controller::snapshots::ConversationRow;

/// What Jump searches: each network's id and display name in sidebar order, and every
/// conversation row. The shell takes a view when the sheet opens, as the sheet holds
/// the list it searches (the old world was cloned the same way).
#[derive(Clone, Debug, Default)]
pub struct JumpView {
    pub networks: Vec<(String, String)>,
    pub conversations: Vec<ConversationRow>,
}

impl JumpView {
    /// The view of the controller's networks and rows right now.
    pub fn of(controller: &Controller) -> Self {
        Self {
            networks: controller
                .network_order()
                .into_iter()
                .map(|id| {
                    let name = controller.network_name(&id);
                    (id, name)
                })
                .collect(),
            conversations: controller.conversations(),
        }
    }
}

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

/// The Jump candidates for `query`, ranked and capped. The candidates are every
/// conversation and each network's Status, in sidebar order, collapsed networks
/// included. Jump takes no collapse state.
pub fn jump_targets(view: &JumpView, query: &str) -> Vec<JumpTarget> {
    let rows = &view.conversations;
    let mut duplicate_names = std::collections::HashMap::<String, usize>::new();
    for row in rows {
        *duplicate_names.entry(row.target.clone()).or_insert(0) += 1;
    }

    let mut candidates = Vec::new();
    for (network_id, network_name) in &view.networks {
        let network_name = network_name.clone();
        let network_id = network_id.clone();
        for row in rows.iter().filter(|row| row.key.network_id == network_id) {
            candidates.push(Candidate {
                kind: JumpKind::Conversation,
                name: row.target.clone(),
                network_id: network_id.clone(),
                conversation_id: row.conversation_id.clone(),
                label: conversation_label(&row.target, &network_name, &duplicate_names),
                detail: row.detail.clone(),
                order: candidates.len(),
            });
        }
        candidates.push(Candidate {
            kind: JumpKind::Status,
            name: "Status".to_string(),
            network_id: network_id.clone(),
            conversation_id: String::new(),
            label: status_label(&network_name),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_omarchy_gets_network_suffix() {
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "omarchy");
        let labels: Vec<_> = targets
            .iter()
            .filter(|t| t.name == "#omarchy")
            .map(|t| t.label.clone())
            .collect();
        assert!(labels.iter().any(|l| l.contains('·')));
    }

    #[test]
    fn unique_names_no_suffix() {
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "ricing");
        let ricing = targets.iter().find(|t| t.name == "#ricing").expect("#ricing");
        assert_eq!(ricing.label, "#ricing");
    }

    #[test]
    fn fuzzy_subsequence_finds_omarchy() {
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "omy");
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
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "build");
        let detail_only = targets
            .iter()
            .find(|t| !t.name_hit && t.name == "#lab")
            .expect("#lab topic-only hit");
        assert!(detail_only.label_indices.is_empty());
    }

    #[test]
    fn blank_query_keeps_sidebar_order_and_marks_no_letters() {
        let controller = Controller::seeded_for_tests();
        let blank = jump_targets(&JumpView::of(&controller), "");
        assert!(blank.iter().all(|t| t.label_indices.is_empty()));
        let orders: Vec<usize> = blank.iter().map(|t| t.order).collect();
        assert!(orders.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn jump_omarchy_picks_the_omarchy_network_first() {
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "#omarchy");
        let first = targets.first().expect("a #omarchy hit");
        assert_eq!(first.name, "#omarchy");
        assert_eq!(first.network_id, "omarchy");
        assert_eq!(first.label, "#omarchy · irc.example · fred");
    }

    #[test]
    fn jump_candidates_follow_sidebar_order_after_a_network_move() {
        let mut controller = Controller::seeded_for_tests();
        assert!(controller.move_network("omarchy", 1));
        let targets = jump_targets(&JumpView::of(&controller), "");
        assert_eq!(targets.first().map(|t| t.network_id.as_str()), Some("oftc"));
        // Jump takes no collapse state, so a collapsed network still lists its rows.
        assert!(jump_targets(&JumpView::of(&controller), "#build").iter().any(|t| t.network_id == "oftc"));
    }

    #[test]
    fn limit_twenty() {
        let controller = Controller::seeded_for_tests();
        let targets = jump_targets(&JumpView::of(&controller), "");
        assert!(targets.len() <= JUMP_RESULT_LIMIT);
    }
}
