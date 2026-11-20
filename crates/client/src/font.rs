use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub(crate) fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::empty();
    let roboto = FontData::from_static(include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"));
    fonts.font_data.insert("Roboto".into(), roboto.into());
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.insert(family, vec!["Roboto".into()]);
    }
    ctx.set_fonts(fonts);
}
