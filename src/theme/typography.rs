use gpui::{Pixels, px};

pub struct TextStyleDef {
    pub size: Pixels,
}

pub struct Typography {
    pub heading: TextStyleDef,
    pub body: TextStyleDef,
    pub code: TextStyleDef,
    pub code_small: TextStyleDef,
}

impl Typography {
    pub fn default() -> Self {
        Self {
            heading: TextStyleDef { size: px(14.0) },
            body: TextStyleDef { size: px(12.0) },
            code: TextStyleDef { size: px(12.0) },
            code_small: TextStyleDef { size: px(11.0) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn test_typography_hierarchy() {
        let t = Typography::default();
        assert_eq!(t.heading.size, px(14.0));
        assert_eq!(t.body.size, px(12.0));
        assert_eq!(t.code.size, px(12.0));
        assert_eq!(t.code_small.size, px(11.0));
    }
}
