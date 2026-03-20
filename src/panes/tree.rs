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
