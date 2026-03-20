pub type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug)]
pub enum PaneTree {
    Leaf(PaneId),
    Branch {
        direction: SplitDirection,
        children: Vec<PaneTree>,
        flex_ratios: Vec<f32>,
    },
}

pub enum CloseResult {
    /// Pane removed, tree still has leaves
    Removed,
    /// Last pane was closed -- window should close
    LastPane,
    /// Pane not found
    NotFound,
}

/// Minimum flex ratio for any pane (prevents invisible panes).
const MIN_FLEX_RATIO: f32 = 0.1;

impl PaneTree {
    /// Split a leaf pane, creating a new sibling pane.
    ///
    /// If the target leaf's parent branch has the same direction, the new pane is
    /// inserted as a sibling (flat). If the direction differs, a new nested branch
    /// is created.
    ///
    /// When splitting a root leaf, a new branch is always created.
    pub fn split(&mut self, target: PaneId, new_id: PaneId, direction: SplitDirection) {
        // Try to split within an existing branch first (same-direction optimization).
        // If that succeeds, we're done.
        if self.split_in_branch(target, new_id, direction) {
            return;
        }

        // Otherwise, handle the case where self is the target leaf directly.
        if let PaneTree::Leaf(id) = self {
            if *id == target {
                let old_leaf = PaneTree::Leaf(target);
                let new_leaf = PaneTree::Leaf(new_id);
                *self = PaneTree::Branch {
                    direction,
                    children: vec![old_leaf, new_leaf],
                    flex_ratios: vec![0.5, 0.5],
                };
            }
        }
    }

    /// Try to split within a branch, handling same-direction sibling insertion.
    /// Returns true if the split was handled.
    fn split_in_branch(
        &mut self,
        target: PaneId,
        new_id: PaneId,
        direction: SplitDirection,
    ) -> bool {
        if let PaneTree::Branch {
            direction: branch_dir,
            children,
            flex_ratios,
        } = self
        {
            // Check if any direct child is the target leaf
            for i in 0..children.len() {
                if let PaneTree::Leaf(id) = &children[i] {
                    if *id == target {
                        if *branch_dir == direction {
                            // Same direction: insert as sibling right after the target
                            children.insert(i + 1, PaneTree::Leaf(new_id));
                            // Redistribute ratios equally
                            let count = children.len();
                            let equal = 1.0 / count as f32;
                            *flex_ratios = vec![equal; count];
                        } else {
                            // Different direction: replace the leaf with a nested branch
                            let old_leaf = PaneTree::Leaf(target);
                            let new_leaf = PaneTree::Leaf(new_id);
                            children[i] = PaneTree::Branch {
                                direction,
                                children: vec![old_leaf, new_leaf],
                                flex_ratios: vec![0.5, 0.5],
                            };
                        }
                        return true;
                    }
                }
            }

            // Recurse into child branches
            for child in children.iter_mut() {
                if child.split_in_branch(target, new_id, direction) {
                    return true;
                }
            }
        }
        false
    }

    /// Close (remove) a pane from the tree.
    ///
    /// Returns `CloseResult::Removed` if the pane was found and removed,
    /// `CloseResult::LastPane` if it was the last remaining pane,
    /// or `CloseResult::NotFound` if the target pane doesn't exist.
    pub fn close(&mut self, target: PaneId) -> CloseResult {
        // Handle root leaf case
        if let PaneTree::Leaf(id) = self {
            if *id == target {
                return CloseResult::LastPane;
            } else {
                return CloseResult::NotFound;
            }
        }

        // Try to remove from branch
        if self.remove_from_branch(target) {
            // After removal, collapse single-child branches at the root
            self.collapse_single_child();
            CloseResult::Removed
        } else {
            CloseResult::NotFound
        }
    }

    /// Remove a leaf from a branch, returning true if found and removed.
    fn remove_from_branch(&mut self, target: PaneId) -> bool {
        if let PaneTree::Branch {
            children,
            flex_ratios,
            ..
        } = self
        {
            // Check direct children
            for i in 0..children.len() {
                if let PaneTree::Leaf(id) = &children[i] {
                    if *id == target {
                        children.remove(i);
                        // Redistribute ratios equally
                        let count = children.len();
                        let equal = 1.0 / count as f32;
                        *flex_ratios = vec![equal; count];
                        return true;
                    }
                }
            }

            // Recurse into child branches
            for child in children.iter_mut() {
                if child.remove_from_branch(target) {
                    child.collapse_single_child();
                    return true;
                }
            }
        }
        false
    }

    /// If this node is a branch with exactly one child, replace self with that child.
    fn collapse_single_child(&mut self) {
        let should_collapse = matches!(
            self,
            PaneTree::Branch { children, .. } if children.len() == 1
        );
        if should_collapse {
            if let PaneTree::Branch { mut children, .. } =
                std::mem::replace(self, PaneTree::Leaf(0))
            {
                *self = children.remove(0);
            }
        }
    }

    /// Collect all leaf PaneIds in depth-first left-to-right order.
    pub fn flatten(&self) -> Vec<PaneId> {
        match self {
            PaneTree::Leaf(id) => vec![*id],
            PaneTree::Branch { children, .. } => {
                children.iter().flat_map(|c| c.flatten()).collect()
            }
        }
    }

    /// Return the next pane in flatten order, wrapping from last to first.
    pub fn next_pane(&self, current: PaneId) -> PaneId {
        let ids = self.flatten();
        let pos = ids.iter().position(|&id| id == current).unwrap_or(0);
        ids[(pos + 1) % ids.len()]
    }

    /// Return the previous pane in flatten order, wrapping from first to last.
    pub fn prev_pane(&self, current: PaneId) -> PaneId {
        let ids = self.flatten();
        let pos = ids.iter().position(|&id| id == current).unwrap_or(0);
        if pos == 0 {
            ids[ids.len() - 1]
        } else {
            ids[pos - 1]
        }
    }

    /// Returns true if the given PaneId exists as a leaf in the tree.
    pub fn find(&self, target: PaneId) -> bool {
        match self {
            PaneTree::Leaf(id) => *id == target,
            PaneTree::Branch { children, .. } => children.iter().any(|c| c.find(target)),
        }
    }

    /// Adjust flex ratios for a branch at the given path.
    ///
    /// `branch_path` is a series of child indices leading to the target branch.
    /// An empty path means the root node. `delta` is shifted from `child_index`
    /// to `child_index + 1`. Minimum ratio is clamped at `MIN_FLEX_RATIO`.
    pub fn update_flex_ratio(&mut self, branch_path: &[usize], child_index: usize, delta: f32) {
        let node = self.node_at_path(branch_path);

        if let Some(PaneTree::Branch { flex_ratios, .. }) = node {
            if child_index + 1 < flex_ratios.len() {
                let mut left = flex_ratios[child_index] - delta;
                let mut right = flex_ratios[child_index + 1] + delta;

                // Clamp minimums
                if left < MIN_FLEX_RATIO {
                    let correction = MIN_FLEX_RATIO - left;
                    left = MIN_FLEX_RATIO;
                    right -= correction;
                }
                if right < MIN_FLEX_RATIO {
                    let correction = MIN_FLEX_RATIO - right;
                    right = MIN_FLEX_RATIO;
                    left -= correction;
                }

                flex_ratios[child_index] = left;
                flex_ratios[child_index + 1] = right;
            }
        }
    }

    /// Navigate to a node at the given path of child indices.
    fn node_at_path(&mut self, path: &[usize]) -> Option<&mut PaneTree> {
        if path.is_empty() {
            return Some(self);
        }

        if let PaneTree::Branch { children, .. } = self {
            if path[0] < children.len() {
                return children[path[0]].node_at_path(&path[1..]);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_vertical_single_leaf() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        // Should create Branch { Vertical, [Leaf(0), Leaf(1)], [0.5, 0.5] }
        match &tree {
            PaneTree::Branch {
                direction,
                children,
                flex_ratios,
            } => {
                assert_eq!(*direction, SplitDirection::Vertical);
                assert_eq!(children.len(), 2);
                assert!(matches!(&children[0], PaneTree::Leaf(0)));
                assert!(matches!(&children[1], PaneTree::Leaf(1)));
                assert_eq!(flex_ratios, &[0.5, 0.5]);
            }
            _ => panic!("Expected Branch after split"),
        }
    }

    #[test]
    fn test_split_horizontal_single_leaf() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Horizontal);
        match &tree {
            PaneTree::Branch {
                direction,
                children,
                flex_ratios,
            } => {
                assert_eq!(*direction, SplitDirection::Horizontal);
                assert_eq!(children.len(), 2);
                assert!(matches!(&children[0], PaneTree::Leaf(0)));
                assert!(matches!(&children[1], PaneTree::Leaf(1)));
                assert_eq!(flex_ratios, &[0.5, 0.5]);
            }
            _ => panic!("Expected Branch after split"),
        }
    }

    #[test]
    fn test_split_same_direction_inserts_sibling() {
        // Start: Branch { Vertical, [Leaf(0), Leaf(1)] }
        // Split Leaf(1) vertically -> should insert sibling, not nest
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Vertical);

        match &tree {
            PaneTree::Branch {
                direction,
                children,
                flex_ratios,
            } => {
                assert_eq!(*direction, SplitDirection::Vertical);
                assert_eq!(children.len(), 3);
                assert!(matches!(&children[0], PaneTree::Leaf(0)));
                assert!(matches!(&children[1], PaneTree::Leaf(1)));
                assert!(matches!(&children[2], PaneTree::Leaf(2)));
                // Ratios should be redistributed equally
                let expected = 1.0 / 3.0;
                for ratio in flex_ratios {
                    assert!((ratio - expected).abs() < 0.01);
                }
            }
            _ => panic!("Expected flat Branch with 3 children"),
        }
    }

    #[test]
    fn test_split_different_direction_nests() {
        // Start: Branch { Vertical, [Leaf(0), Leaf(1)] }
        // Split Leaf(1) horizontally -> should nest a new Horizontal branch
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Horizontal);

        match &tree {
            PaneTree::Branch {
                direction,
                children,
                ..
            } => {
                assert_eq!(*direction, SplitDirection::Vertical);
                assert_eq!(children.len(), 2);
                assert!(matches!(&children[0], PaneTree::Leaf(0)));
                match &children[1] {
                    PaneTree::Branch {
                        direction: inner_dir,
                        children: inner_children,
                        flex_ratios: inner_ratios,
                    } => {
                        assert_eq!(*inner_dir, SplitDirection::Horizontal);
                        assert_eq!(inner_children.len(), 2);
                        assert!(matches!(&inner_children[0], PaneTree::Leaf(1)));
                        assert!(matches!(&inner_children[1], PaneTree::Leaf(2)));
                        assert_eq!(inner_ratios, &[0.5, 0.5]);
                    }
                    _ => panic!("Expected nested Branch"),
                }
            }
            _ => panic!("Expected outer Branch"),
        }
    }

    #[test]
    fn test_close_from_two_child_branch_collapses() {
        // Branch { [Leaf(0), Leaf(1)] } -> close Leaf(1) -> Leaf(0)
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        let result = tree.close(1);
        assert!(matches!(result, CloseResult::Removed));
        assert!(matches!(&tree, PaneTree::Leaf(0)));
    }

    #[test]
    fn test_close_redistributes_flex_ratios() {
        // 3-child branch -> close one -> 2-child branch with [0.5, 0.5]
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Vertical);
        // Now 3 children: [Leaf(0), Leaf(1), Leaf(2)]
        let result = tree.close(1);
        assert!(matches!(result, CloseResult::Removed));
        match &tree {
            PaneTree::Branch {
                children,
                flex_ratios,
                ..
            } => {
                assert_eq!(children.len(), 2);
                assert_eq!(flex_ratios, &[0.5, 0.5]);
            }
            _ => panic!("Expected Branch with 2 children"),
        }
    }

    #[test]
    fn test_close_last_pane_returns_last_pane() {
        let mut tree = PaneTree::Leaf(0);
        let result = tree.close(0);
        assert!(matches!(result, CloseResult::LastPane));
    }

    #[test]
    fn test_flatten_depth_first_left_to_right() {
        // Build a tree: Branch { V, [Leaf(0), Branch { H, [Leaf(1), Leaf(2)] }] }
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Horizontal);
        let ids = tree.flatten();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn test_next_pane_wraps() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Vertical);
        // Flatten: [0, 1, 2]
        assert_eq!(tree.next_pane(0), 1);
        assert_eq!(tree.next_pane(1), 2);
        assert_eq!(tree.next_pane(2), 0); // wraps
    }

    #[test]
    fn test_prev_pane_wraps() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        tree.split(1, 2, SplitDirection::Vertical);
        // Flatten: [0, 1, 2]
        assert_eq!(tree.prev_pane(2), 1);
        assert_eq!(tree.prev_pane(1), 0);
        assert_eq!(tree.prev_pane(0), 2); // wraps
    }

    #[test]
    fn test_find_pane() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        assert!(tree.find(0));
        assert!(tree.find(1));
        assert!(!tree.find(99));
    }

    #[test]
    fn test_update_flex_ratio() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        // Branch at root with [0.5, 0.5]
        // Shift 0.1 from child 0 to child 1
        tree.update_flex_ratio(&[], 0, 0.1);
        match &tree {
            PaneTree::Branch { flex_ratios, .. } => {
                assert!((flex_ratios[0] - 0.4).abs() < 0.001);
                assert!((flex_ratios[1] - 0.6).abs() < 0.001);
            }
            _ => panic!("Expected Branch"),
        }
    }

    #[test]
    fn test_update_flex_ratio_clamps_minimum() {
        let mut tree = PaneTree::Leaf(0);
        tree.split(0, 1, SplitDirection::Vertical);
        // Try to shift 0.5 (would make child 0 = 0.0), should clamp at 0.1
        tree.update_flex_ratio(&[], 0, 0.5);
        match &tree {
            PaneTree::Branch { flex_ratios, .. } => {
                assert!(flex_ratios[0] >= 0.1);
                assert!(flex_ratios[1] <= 0.9);
                assert!((flex_ratios[0] + flex_ratios[1] - 1.0).abs() < 0.001);
            }
            _ => panic!("Expected Branch"),
        }
    }

    #[test]
    fn test_close_not_found() {
        let mut tree = PaneTree::Leaf(0);
        let result = tree.close(99);
        assert!(matches!(result, CloseResult::NotFound));
    }
}
