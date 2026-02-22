//! SearchResultListWidget -- scrollable, selectable list of app results.

use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VCustomButton, VNode};
use crate::widgets::app_result_row::AppResultRowProps;

/// Output events from the search result list.
#[derive(Debug, Clone)]
pub enum SearchResultListOutput {
    /// An item was activated (clicked).
    Activated(usize),
}

/// Props for the search result list.
#[derive(Clone, PartialEq)]
pub struct SearchResultListProps {
    pub items: Vec<AppResultRowProps>,
    pub selected: usize,
}

/// Renders a vertical list of app result buttons with selection highlighting.
pub struct SearchResultListRender;

impl RenderFn for SearchResultListRender {
    type Props = SearchResultListProps;
    type Output = SearchResultListOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<SearchResultListOutput>) -> VNode {
        let mut list = VBox::vertical(0).css_class("search-result-list");

        for (index, item_props) in props.items.iter().enumerate() {
            let mut btn = VCustomButton::new(
                VNode::new::<super::app_result_row::AppResultRowWidget>(item_props.clone()),
            )
            .css_class("app-result-btn");

            if index == props.selected {
                btn = btn.css_class("selected");
            }

            let emit = emit.clone();
            btn = btn.on_click(move || {
                if let Some(ref cb) = *emit.borrow() {
                    cb(SearchResultListOutput::Activated(index));
                }
            });

            list = list.child(VNode::custom_button(btn).key(format!("item-{index}")));
        }

        VNode::vbox(list)
    }
}

/// Type alias preserving the old name for callers.
pub type SearchResultListWidget = RenderComponent<SearchResultListRender>;
