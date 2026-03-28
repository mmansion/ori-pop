//! Typed parameter atoms — the primitive values that flow through the design tree.

use serde::{Deserialize, Serialize};

// ── Value ─────────────────────────────────────────────────────────────────────

/// A typed value that can appear as a node parameter or port payload.
///
/// All variants are serializable and comparable, making the design tree
/// fully portable and agent-readable.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Float(f32),
    Int(i32),
    Uint(u32),
    Bool(bool),
    /// 2D vector — UV coordinate, 2D offset, etc.
    Vec2([f32; 2]),
    /// 3D point or direction in Z-up world space.
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    /// Column-major 4×4 transform matrix.
    Mat4([[f32; 4]; 4]),
    /// UTF-8 string — labels, shader source, file paths.
    Text(String),
    /// Ordered list of values — curve control points, palette entries, etc.
    List(Vec<Value>),
}

impl Value {
    pub fn as_float(&self) -> Option<f32> {
        if let Value::Float(v) = self { Some(*v) } else { None }
    }
    pub fn as_uint(&self) -> Option<u32> {
        if let Value::Uint(v) = self { Some(*v) } else { None }
    }
    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        if let Value::Vec3(v) = self { Some(*v) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(v) = self { Some(*v) } else { None }
    }
    pub fn as_text(&self) -> Option<&str> {
        if let Value::Text(v) = self { Some(v) } else { None }
    }
}

impl From<f32>        for Value { fn from(v: f32)        -> Self { Value::Float(v) } }
impl From<i32>        for Value { fn from(v: i32)        -> Self { Value::Int(v)   } }
impl From<u32>        for Value { fn from(v: u32)        -> Self { Value::Uint(v)  } }
impl From<bool>       for Value { fn from(v: bool)       -> Self { Value::Bool(v)  } }
impl From<[f32; 2]>   for Value { fn from(v: [f32; 2])   -> Self { Value::Vec2(v)  } }
impl From<[f32; 3]>   for Value { fn from(v: [f32; 3])   -> Self { Value::Vec3(v)  } }
impl From<[f32; 4]>   for Value { fn from(v: [f32; 4])   -> Self { Value::Vec4(v)  } }
impl From<String>     for Value { fn from(v: String)     -> Self { Value::Text(v)  } }
impl From<&str>       for Value { fn from(v: &str)       -> Self { Value::Text(v.to_owned()) } }

// ── Domain ────────────────────────────────────────────────────────────────────

/// The allowed range of a numeric parameter.
///
/// Used by the egui inspector for slider bounds and by the agentic layer
/// to understand the valid search space for a parameter.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Domain {
    pub min:  Option<Value>,
    pub max:  Option<Value>,
    /// Discrete step size — `None` means continuous.
    pub step: Option<Value>,
}

impl Domain {
    pub fn float(min: f32, max: f32) -> Self {
        Self { min: Some(Value::Float(min)), max: Some(Value::Float(max)), step: None }
    }
    pub fn uint(min: u32, max: u32) -> Self {
        Self { min: Some(Value::Uint(min)), max: Some(Value::Uint(max)), step: Some(Value::Uint(1)) }
    }
    pub fn positive() -> Self {
        Self { min: Some(Value::Float(0.0)), max: None, step: None }
    }
}

// ── Param ─────────────────────────────────────────────────────────────────────

/// A named, typed, documented parameter on a design-tree node.
///
/// `domain` and `doc` make every parameter legible to a human in the inspector
/// and to an AI agent without additional context.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Param {
    /// Machine-readable identifier — snake_case, stable across versions.
    pub name:   String,
    /// Current value.
    pub value:  Value,
    /// Valid range, for inspector UI and agent guidance.
    pub domain: Option<Domain>,
    /// Human and agent readable description of what this parameter does.
    pub doc:    Option<String>,
}

impl Param {
    pub fn new(name: impl Into<String>, value: impl Into<Value>) -> Self {
        Self { name: name.into(), value: value.into(), domain: None, doc: None }
    }

    pub fn with_domain(mut self, domain: Domain) -> Self {
        self.domain = Some(domain);
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_round_trips_json() {
        let v = Value::Vec3([1.0, 2.0, 3.0]);
        let json = serde_json::to_string(&v).unwrap();
        let back: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn param_builder() {
        let p = Param::new("radius", 1.0_f32)
            .with_domain(Domain::float(0.01, 100.0))
            .with_doc("Sphere radius in world units (Z-up).");
        assert_eq!(p.name, "radius");
        assert_eq!(p.value.as_float(), Some(1.0));
    }
}
