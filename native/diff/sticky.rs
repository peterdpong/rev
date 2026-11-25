use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ListState, Pixels, Position, Style, Window, point,
    px, relative, size,
};

type HeaderRenderer = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

pub(crate) struct StickyHeader {
    render: Option<HeaderRenderer>,
    header: Option<AnyElement>,
    list: ListState,
    next: Option<usize>,
}

impl StickyHeader {
    pub(crate) fn new(
        render: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
        list: ListState,
        next: Option<usize>,
    ) -> Self {
        Self {
            render: Some(Box::new(render)),
            header: None,
            list,
            next,
        }
    }
}

impl IntoElement for StickyHeader {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StickyHeader {
    type RequestLayoutState = ();
    type PrepaintState = Option<ContentMask<Pixels>>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.position = Position::Absolute;
        style.inset.top = px(0.).into();
        style.inset.left = px(0.).into();
        style.size.width = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let width = bounds.size.width;
        let Some(render) = self.render.take() else {
            return None;
        };
        let header = self.header.insert(render(window, cx));
        let height = header
            .layout_as_root(
                size(AvailableSpace::Definite(width), AvailableSpace::MinContent),
                window,
                cx,
            )
            .height;

        let mut top = bounds.top();
        if let Some(next) = self.next.and_then(|ix| self.list.bounds_for_item(ix)) {
            top = top.min(next.top() - height);
        }

        let mask = ContentMask {
            bounds: Bounds::new(bounds.origin, size(width, height)),
        };
        window.with_content_mask(Some(mask.clone()), |window| {
            header.prepaint_at(point(bounds.left(), top), window, cx);
        });
        Some(mask)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        mask: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(header) = &mut self.header {
            window.with_content_mask(mask.take(), |window| header.paint(window, cx));
        }
    }
}
