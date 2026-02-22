//! Reusable menu chevron widget.
//!
//! A chevron icon that gains the `expanded` CSS class when expanded,
//! suitable for menus and expandable content.

use crate::icons::Icon;
use crate::vdom::primitives::VIcon;
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// Properties for the menu chevron.
#[derive(Clone, PartialEq, Debug)]
pub struct MenuChevronProps {
    pub expanded: bool,
}

pub struct MenuChevronRender;

impl RenderFn for MenuChevronRender {
    type Props = MenuChevronProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let icon = VIcon::new(vec![Icon::Themed("pan-down-symbolic".to_string())], 16)
            .css_class("menu-chevron")
            .css_classes(props.expanded.then_some("expanded"));
        VNode::icon(icon)
    }
}

pub type MenuChevronWidget = RenderComponent<MenuChevronRender>;
