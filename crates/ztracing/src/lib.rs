pub use ztracing_macro::instrument;

pub struct Span;

impl Span {
    pub fn current() -> Self {
        Self
    }
    pub fn enter(&self) -> Self {
        Self
    }
    pub fn record<Q: ?Sized, V: ?Sized>(&self, _field: &Q, _value: &V) -> &Self {
        self
    }
}

pub fn init() {}
