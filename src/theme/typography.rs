use gpui::{FontWeight, Pixels, SharedString, px};

pub struct TextStyleDef {
    pub size: Pixels,
    pub weight: FontWeight,
}

pub struct Typography {
    pub heading: TextStyleDef,
    pub body: TextStyleDef,
    pub code: TextStyleDef,
    pub code_small: TextStyleDef,
    pub mono_family: SharedString,
}

impl Typography {
    pub fn default() -> Self {
        Self {
            heading: TextStyleDef {
                size: px(14.0),
                weight: FontWeight::SEMIBOLD,
            },
            body: TextStyleDef {
                size: px(12.0),
                weight: FontWeight::NORMAL,
            },
            code: TextStyleDef {
                size: px(12.0),
                weight: FontWeight::NORMAL,
            },
            code_small: TextStyleDef {
                size: px(11.0),
                weight: FontWeight::NORMAL,
            },
            mono_family: SharedString::from("Menlo"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FontWeight, px};

    #[test]
    fn test_typography_hierarchy() {
        let t = Typography::default();
        assert_eq!(t.heading.size, px(14.0));
        assert_eq!(t.heading.weight, FontWeight::SEMIBOLD);
        assert_eq!(t.body.size, px(12.0));
        assert_eq!(t.body.weight, FontWeight::NORMAL);
        assert_eq!(t.code.size, px(12.0));
        assert_eq!(t.code_small.size, px(11.0));
    }

    #[test]
    fn test_mono_family_is_menlo() {
        let t = Typography::default();
        assert_eq!(t.mono_family.as_ref(), "Menlo");
    }
}
