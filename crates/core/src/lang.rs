use crate::types::Language;
use regex::Regex;

pub const CODE_EXTS: &[&str] = &["swift", "kt", "kts"];

pub const RESOURCE_EXTS: &[&str] = &[
    "xib",
    "storyboard",
    "plist",
    "strings",
    "stringsdict",
    "entitlements",
    "xml",
    "json",
    "xcstrings",
];

pub const SKIP_DIRS: &[&str] = &[
    ".git",
    "build",
    ".build",
    "DerivedData",
    "Pods",
    "Carthage",
    "node_modules",
    ".gradle",
    ".idea",
    ".swiftpm",
    "out",
    "generated",
];

/// Reachable implicitly in Swift/SwiftUI/UIKit or Kotlin/Compose/Android.
pub const NOISE: &[&str] = &[
    "body",
    "init",
    "deinit",
    "main",
    "description",
    "debugDescription",
    "hash",
    "hashValue",
    "encode",
    "decode",
    "CodingKeys",
    "makeUIView",
    "updateUIView",
    "makeUIViewController",
    "updateUIViewController",
    "makeCoordinator",
    "makeNSView",
    "updateNSView",
    "viewDidLoad",
    "viewWillAppear",
    "viewDidAppear",
    "viewWillDisappear",
    "viewDidDisappear",
    "viewDidLayoutSubviews",
    "layoutSubviews",
    "prepareForReuse",
    "awakeFromNib",
    "didMoveToSuperview",
    "application",
    "scene",
    "sceneDidBecomeActive",
    "sceneWillEnterForeground",
    "previews",
    "Preview",
    "PreviewProvider",
    "onCreate",
    "onStart",
    "onResume",
    "onPause",
    "onStop",
    "onDestroy",
    "onCreateView",
    "onViewCreated",
    "onDestroyView",
    "onSaveInstanceState",
    "onBindViewHolder",
    "onCreateViewHolder",
    "getItemCount",
    "onCleared",
    "onBackPressed",
    "onActivityResult",
    "onRequestPermissionsResult",
    "toString",
    "equals",
    "hashCode",
    "compareTo",
    "invoke",
    "areItemsTheSame",
    "areContentsTheSame",
];

/// Attributes that mean "reachable outside the ordinary call graph".
pub const DYNAMIC_ATTRS: &[&str] = &[
    "@objc",
    "@objcMembers",
    "@IBAction",
    "@IBOutlet",
    "@IBInspectable",
    "@IBDesignable",
    "@NSManaged",
    "@main",
    "@UIApplicationMain",
    "@NSApplicationMain",
    "@_cdecl",
    "@_dynamicReplacement",
    "@Preview",
    "@Composable",
    "@Test",
    "@Before",
    "@After",
    "@BeforeEach",
    "@AfterEach",
    "@JvmStatic",
    "@Keep",
    "@Serializable",
    "@Inject",
    "@Provides",
    "@Binds",
    "@HiltViewModel",
    "@AndroidEntryPoint",
    "@JsonClass",
    "@Parcelize",
    "@Entity",
    "@Dao",
    "@Query",
    "@Insert",
];

pub fn language_for_ext(ext: &str) -> Option<Language> {
    match ext {
        "swift" => Some(Language::Swift),
        "kt" | "kts" => Some(Language::Kotlin),
        _ => None,
    }
}

/// Compiled declaration patterns. Built once per scan, not per file.
pub struct Patterns {
    pub swift: Vec<(&'static str, Regex)>,
    pub kotlin: Vec<(&'static str, Regex)>,
    /// Matches a `protocol` / `interface` header line.
    pub conformance_head: Regex,
    /// Matches a member declaration inside such a body.
    pub conformance_member: Regex,
}

impl Patterns {
    pub fn new() -> Self {
        Self {
            swift: swift_patterns(),
            kotlin: kotlin_patterns(),
            conformance_head: Regex::new(
                r"(?m)^[ \t]*(?:[\w@()]+[ \t]+)*(?:protocol|interface)[ \t]+[A-Za-z_][A-Za-z0-9_]*",
            )
            .unwrap(),
            conformance_member: Regex::new(
                r"(?m)^[ \t]*(?:[\w@()]+[ \t]+)*(?:func|fun|var|val)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
            )
            .unwrap(),
        }
    }

    pub fn for_language(&self, lang: Language) -> &[(&'static str, Regex)] {
        match lang {
            Language::Swift => &self.swift,
            Language::Kotlin => &self.kotlin,
        }
    }
}

impl Default for Patterns {
    fn default() -> Self {
        Self::new()
    }
}

fn swift_patterns() -> Vec<(&'static str, Regex)> {
    let m = r"(?:public|private|internal|fileprivate|open|static|final|class|mutating|nonmutating|lazy|weak|unowned|convenience|required|indirect|dynamic|async|override|@\w+(?:\([^)]*\))?)";
    vec![
        (
            "func",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*func[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "class",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*class[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]*[:{{<]"
            )),
        ),
        (
            "struct",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*struct[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "enum",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*enum[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "protocol",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*protocol[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "typealias",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*typealias[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "actor",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*actor[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "property",
            re(&format!(
                r"(?m)^[ \t]{{0,4}}(?:{m}[ \t]+)*(?:let|var)[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "case",
            re(r"(?m)^[ \t]*case[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]*(?:[,(=]|$)"),
        ),
    ]
}

fn kotlin_patterns() -> Vec<(&'static str, Regex)> {
    let m = r"(?:public|private|internal|protected|open|abstract|sealed|data|inline|suspend|external|expect|actual|value|inner|companion|const|lateinit|operator|infix|tailrec|override|@\w+(?:\([^)]*\))?)";
    vec![
        (
            "fun",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*fun[ \t]+(?:<[^>]*>[ \t]*)?([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "class",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*class[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "interface",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*interface[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "object",
            re(&format!(
                r"(?m)^[ \t]*(?:{m}[ \t]+)*object[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "typealias",
            re(r"(?m)^[ \t]*typealias[ \t]+([A-Za-z_][A-Za-z0-9_]*)"),
        ),
        (
            "property",
            re(&format!(
                r"(?m)^[ \t]{{0,4}}(?:{m}[ \t]+)*(?:val|var)[ \t]+([A-Za-z_][A-Za-z0-9_]*)"
            )),
        ),
        (
            "enum entry",
            re(r"(?m)^[ \t]{4,}([A-Z][A-Z0-9_]{2,})[ \t]*(?:[,;(]|$)"),
        ),
    ]
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("built-in pattern must compile")
}
