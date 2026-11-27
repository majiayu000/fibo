//! The input method's candidates go where the text is being composed, not under the box.
use super::Field;
use gpui::{
    Bounds, Entity, EntityInputHandler, IntoElement, Render, TestAppContext, Window, div,
    prelude::*, px,
};

struct Form {
    field: Entity<Field>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(400.)).child(self.field.clone())
    }
}

#[gpui::test]
fn candidates_follow_the_text_being_composed(cx: &mut TestAppContext) {
    let (form, cx) = cx.add_window_view(|_, cx| Form {
        field: cx.new(|cx| Field::new("描述", "第一行\n第二行\n第三行", cx)),
    });
    cx.run_until_parked();
    let field = form.read_with(cx, |f, _| f.field.clone());
    let whole = Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(px(400.), px(300.)));
    // UTF-16 units, as the input method counts: 一 is 1..2, 三 is 9..10 and 第 before it 8..9.
    let [first, third_start, third] = [1..2, 8..9, 9..10].map(|range| {
        field.update_in(cx, |f, window, cx| {
            f.bounds_for_range(range, whole, window, cx)
        })
    });
    let (first, third_start, third) = (first.unwrap(), third_start.unwrap(), third.unwrap());
    assert_ne!(third, whole, "the whole box, not the composed text");
    assert!(third.origin.y > first.origin.y, "{first:?} {third:?}");
    assert!(third.size.height < whole.size.height / 2., "{third:?}");
    assert!(
        third.origin.x > third_start.origin.x,
        "{third_start:?} {third:?}"
    );
}
