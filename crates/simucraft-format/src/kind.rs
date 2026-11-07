use std::path::Path;

/// The format version this crate reads and writes.
pub const FORMAT_VERSION: &str = "0.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Project,
    Lock,
    Model,
    Layout,
    Experiment,
    Design,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Project,
        Kind::Lock,
        Kind::Model,
        Kind::Layout,
        Kind::Experiment,
        Kind::Design,
    ];

    /// The `<kind>` in `format = "simucraft.<kind>/<version>"`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Project => "project",
            Kind::Lock => "lock",
            Kind::Model => "model",
            Kind::Layout => "layout",
            Kind::Experiment => "experiment",
            Kind::Design => "design",
        }
    }

    pub fn from_path(path: &Path) -> Option<Kind> {
        match path.file_name()?.to_str()? {
            "simucraft.toml" => return Some(Kind::Project),
            "simucraft.lock" => return Some(Kind::Lock),
            _ => {}
        }
        match path.extension()?.to_str()? {
            "scmodel" => Some(Kind::Model),
            "sclayout" => Some(Kind::Layout),
            "scexp" => Some(Kind::Experiment),
            "scdesign" => Some(Kind::Design),
            _ => None,
        }
    }
}
