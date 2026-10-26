//! Port of `tui/internal/demo/seed.go` and the seed part of
//! `tui/internal/demo/demo.go`: the two demo networks, omarchy and oftc.
//!
//! The data is content- and order-identical to the Go and C++ seeds. The
//! containers differ only in shape: slices for QVector and QStringList, and
//! `RankPair` for QPair. Everything is `'static` const data, so the seed costs
//! nothing to build and every builder in `wire` and `server` reads it by
//! reference.
//!
//! The one place the Go port computes a value is the `#ricing` unread pad. The
//! pad lines are written out here instead, and a test checks that the channel
//! still holds the twelve lines the parity world expects.

/// The primary demo network's id (`omarchyNetworkId()`).
pub const OMARCHY_NETWORK_ID: &str = "omarchy";
/// The secondary demo network's id (`oftcNetworkId()`).
pub const OFTC_NETWORK_ID: &str = "oftc";

/// The demo clock's fixed day, used verbatim as the server-time tag date.
pub const TODAY: &str = "2026-09-12";
/// The day before `TODAY`, for the seeded #ricing and dax direct lines.
pub const YESTERDAY: &str = "2026-09-11";

/// The CAP LS and CAP ACK list the demo server advertises.
pub const CAPS: &str = concat!(
    "echo-message message-tags labeled-response away-notify multi-prefix batch ",
    "draft/metadata-2 server-time account-tag account-notify extended-join",
);

/// The 005 tokens the demo server advertises.
pub const ISUPPORT: &str = "CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100";

/// The MONITOR entry ceiling, mirroring `IrcServerFeatures`.
pub const MONITOR_LIMIT: usize = 100;

/// The operator's own avatar, advertised for both seeded networks. Real HTTPS on
/// purpose: the identity footer shows the Omairc mark, and the demo exercises
/// the avatar store's network path. See `wire::presence_lines`.
pub const DEMO_SELF_AVATAR_URL: &str = "https://omairc.app/omairc128.png";

/// Whether a seed line is a PRIVMSG row or a JOIN row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedKind {
    /// A PRIVMSG row.
    Chat,
    /// A JOIN row; only `nick` and `account` are meaningful.
    Join,
}

/// One transcript row (`SeedLine`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedLine {
    pub kind: SeedKind,
    pub nick: &'static str,
    pub body: &'static str,
    pub day: &'static str,
    pub hhmm: &'static str,
    /// The account tag, or empty.
    pub account: &'static str,
}

/// A nick paired with a PREFIX rank or a status value (`RankPair`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankPair {
    pub nick: &'static str,
    pub value: &'static str,
}

/// One seeded channel (`SeedChannel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedChannel {
    pub name: &'static str,
    pub topic: &'static str,
    pub members: &'static [&'static str],
    /// Members who join through the transcript, left out of the initial NAMES.
    pub late_join: &'static [&'static str],
    pub away: &'static [&'static str],
    pub ranks: &'static [RankPair],
    pub statuses: &'static [RankPair],
    pub lines: &'static [SeedLine],
    pub mark_read: bool,
}

/// One seeded direct message conversation (`SeedDirect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedDirect {
    pub nick: &'static str,
    pub lines: &'static [SeedLine],
    pub mark_read: bool,
    /// Whether the peer's typing hint is injected at attach.
    pub typing: bool,
}

/// One `/list` directory entry (`SeedListed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedListed {
    pub name: &'static str,
    pub users: usize,
    pub topic: &'static str,
}

/// One seeded network (`SeedNetwork`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedNetwork {
    pub network_id: &'static str,
    pub nick: &'static str,
    pub welcome: &'static str,
    /// The `draft/ICON` value advertised in 005, or empty.
    pub icon_url: &'static str,
    pub channels: &'static [SeedChannel],
    pub directs: &'static [SeedDirect],
    pub directory: &'static [SeedListed],
}

const fn chat(nick: &'static str, body: &'static str, hhmm: &'static str) -> SeedLine {
    chat_on(nick, body, hhmm, TODAY)
}

const fn chat_on(
    nick: &'static str,
    body: &'static str,
    hhmm: &'static str,
    day: &'static str,
) -> SeedLine {
    SeedLine {
        kind: SeedKind::Chat,
        nick,
        body,
        day,
        hhmm,
        account: "",
    }
}

const fn chat_account(
    nick: &'static str,
    body: &'static str,
    hhmm: &'static str,
    day: &'static str,
    account: &'static str,
) -> SeedLine {
    SeedLine {
        kind: SeedKind::Chat,
        nick,
        body,
        day,
        hhmm,
        account,
    }
}

const fn join(nick: &'static str) -> SeedLine {
    join_account(nick, "")
}

const fn join_account(nick: &'static str, account: &'static str) -> SeedLine {
    SeedLine {
        kind: SeedKind::Join,
        nick,
        body: "",
        day: "",
        hhmm: "",
        account,
    }
}

const fn rank(nick: &'static str, value: &'static str) -> RankPair {
    RankPair { nick, value }
}

// ---- omarchy ------------------------------------------------------------

const OMARCHY_MAIN_LINES: &[SeedLine] = &[
    chat(
        "anna",
        "Morning! Has anyone tried the new minimal install flow yet?",
        "09:41",
    ),
    chat(
        "dax",
        "Yes. Fresh install on my Framework took about twelve minutes. The defaults feel really considered.",
        "09:43",
    ),
    chat(
        "mira",
        "The way the theme carries across the terminal and native apps is my favorite detail.",
        "09:46",
    ),
    join_account("sol", "solarius"),
    chat("sol", "Hey all. Just landed here from Arch. This feels surprisingly calm.", "09:52"),
    chat("anna", "Welcome, sol. Calm is the whole idea.", "09:53"),
    chat(
        "dax",
        "If you have not already, try the keyboard-first app launcher. It becomes muscle memory fast.",
        "09:55",
    ),
    chat("sol", "I found it. The shortcuts sheet is a nice touch too.", "09:56"),
    chat(
        "mira",
        "Most of the system makes sense once you learn three or four core bindings.",
        "09:57",
    ),
    chat(
        "anna",
        "And everything important is still plain text when you want to look underneath.",
        "09:58",
    ),
    chat_account(
        "kai",
        "That balance is hard to get right: friendly defaults without hiding the actual system.",
        "09:59",
        TODAY,
        "kaidev",
    ),
    chat(
        "dax",
        "Exactly. Start simple, then make it yours one deliberate change at a time.",
        "10:00",
    ),
    join("nora"),
    chat(
        "nora",
        "Good timing. I was just looking for a quiet place to ask about native Omarchy apps.",
        "10:01",
    ),
    chat(
        "fred",
        "I am sketching a tiny IRC client that belongs here. No browser chrome, no clutter.",
        "10:02",
    ),
    chat("mira", "Keep the member list optional and I am sold.", "10:04"),
];

const OMARCHY_DESKTOP_LINES: &[SeedLine] = &[
    chat(
        "dax",
        "I finally moved every workspace rule into a small, readable file.",
        "08:22",
    ),
    chat(
        "mira",
        "That is the dream. Configuration you can understand in one sitting.",
        "08:24",
    ),
    chat(
        "sol",
        "Does anyone use a vertical monitor alongside the main display?",
        "10:08",
    ),
];

// The unread target is twelve: three seeded lines, eight pads, and one line
// that mentions fred. The pads are written out so the table stays const.
const OMARCHY_RICING_LINES: &[SeedLine] = &[
    chat_on(
        "anna",
        "Muted colors, one strong accent, and enough breathing room.",
        "18:10",
        YESTERDAY,
    ),
    chat_on(
        "mira",
        "Typography does more work than decoration ever will.",
        "18:13",
        YESTERDAY,
    ),
    chat_on(
        "dax",
        "Dropped a new warm theme in the usual place. It looks great after sunset.",
        "18:20",
        YESTERDAY,
    ),
    chat_on("anna", "rice-pad-1", "18:21", YESTERDAY),
    chat_on("anna", "rice-pad-2", "18:22", YESTERDAY),
    chat_on("anna", "rice-pad-3", "18:23", YESTERDAY),
    chat_on("anna", "rice-pad-4", "18:24", YESTERDAY),
    chat_on("anna", "rice-pad-5", "18:25", YESTERDAY),
    chat_on("anna", "rice-pad-6", "18:26", YESTERDAY),
    chat_on("anna", "rice-pad-7", "18:27", YESTERDAY),
    chat_on("anna", "rice-pad-8", "18:28", YESTERDAY),
    chat_on("mira", "looks good, fred.", "18:29", YESTERDAY),
];

const OMARCHY_HELP_LINES: &[SeedLine] = &[
    chat(
        "mira",
        "Tip: include the command output and the exact behavior you expected.",
        "09:11",
    ),
    chat(
        "sol",
        "That made my monitor issue much easier to diagnose. Thanks.",
        "09:15",
    ),
];

const OMARCHY_ANNA_DIRECT_LINES: &[SeedLine] = &[chat(
    "anna",
    "fred: The prototype already feels at home. Nice work.",
    "10:12",
)];

const OMARCHY_DAX_DIRECT_LINES: &[SeedLine] = &[chat_on(
    "dax",
    "Send me the build when the mock is ready.",
    "18:40",
    YESTERDAY,
)];

const OMARCHY_MEMBERS: &[&str] = &[
    "anna", "dax", "mira", "sol", "fred", "kai", "nora", "teo", "lena", "sam", "ivy", "max",
];
const OMARCHY_LATE_JOIN: &[&str] = &["sol", "nora"];
const OMARCHY_AWAY: &[&str] = &["teo", "lena", "sam", "ivy", "max"];

// The demo ladder: founder, admin, two ops, halfop, voice, then the rest plain.
// mira holds two ranks at once, which multi-prefix allows, and teo is away and
// voiced, because rank and presence are independent.
//
// The late joiners arrive through the transcript, and an arriving member
// carries no rank until a server says so, so they stay out of this list and out
// of the initial NAMES.
const OMARCHY_RANKS: &[RankPair] = &[
    rank("fred", "~"),
    rank("anna", "&"),
    rank("dax", "@"),
    rank("mira", "@+"),
    rank("kai", "%"),
    rank("teo", "+"),
];

const OMARCHY_STATUSES: &[RankPair] = &[
    rank("anna", "writing docs"),
    rank("dax", "on #desktop"),
    rank("mira", "making tea"),
    rank("sol", "new here"),
    rank("fred", "building Omairc"),
];

const OMARCHY_DESKTOP_MEMBERS: &[&str] =
    &["anna", "dax", "mira", "sol", "fred", "kai", "nora", "teo"];
const OMARCHY_RICING_MEMBERS: &[&str] = &[
    "anna", "dax", "mira", "sol", "fred", "kai", "nora", "teo", "lena", "sam",
];
const OMARCHY_HELP_MEMBERS: &[&str] = &["anna", "dax", "mira", "sol", "fred"];

const OMARCHY_CHANNELS: &[SeedChannel] = &[
    SeedChannel {
        name: "#omarchy",
        topic: "A cozy corner for Omarchy users and builders.",
        members: OMARCHY_MEMBERS,
        late_join: OMARCHY_LATE_JOIN,
        away: OMARCHY_AWAY,
        ranks: OMARCHY_RANKS,
        statuses: OMARCHY_STATUSES,
        lines: OMARCHY_MAIN_LINES,
        mark_read: false,
    },
    SeedChannel {
        name: "#desktop",
        topic: "Desktops should feel personal, fast, and calm.",
        members: OMARCHY_DESKTOP_MEMBERS,
        late_join: &[],
        away: &[],
        ranks: &[],
        statuses: &[],
        lines: OMARCHY_DESKTOP_LINES,
        mark_read: false,
    },
    SeedChannel {
        name: "#ricing",
        topic: "Themes, type, wallpapers, and the tiny details.",
        members: OMARCHY_RICING_MEMBERS,
        late_join: &[],
        away: &[],
        ranks: &[],
        statuses: &[],
        lines: OMARCHY_RICING_LINES,
        mark_read: false,
    },
    SeedChannel {
        name: "#help",
        topic: "Ask a clear question. Share what you already tried.",
        members: OMARCHY_HELP_MEMBERS,
        late_join: &[],
        away: &[],
        ranks: &[],
        statuses: &[],
        lines: OMARCHY_HELP_LINES,
        mark_read: true,
    },
];

const OMARCHY_DIRECTS: &[SeedDirect] = &[
    SeedDirect {
        nick: "anna",
        lines: OMARCHY_ANNA_DIRECT_LINES,
        mark_read: false,
        typing: true,
    },
    SeedDirect {
        nick: "dax",
        lines: OMARCHY_DAX_DIRECT_LINES,
        mark_read: true,
        typing: false,
    },
];

const OMARCHY_DIRECTORY: &[SeedListed] = &[
    SeedListed {
        // Literal control bytes: STX around "Kernel", then mIRC color 04 around
        // "distro", then a bare ETX reset.
        name: "#linux",
        users: 42,
        topic: concat!(
            "\x02Kernel\x02 discussion and \x03",
            "04distro",
            "\x03 help.",
        ),
    },
    SeedListed {
        name: "#random",
        users: 4,
        topic: "Off-topic chatter.",
    },
];

/// The primary demo network (`omarchyWorld()`).
pub static OMARCHY_WORLD: SeedNetwork = SeedNetwork {
    network_id: OMARCHY_NETWORK_ID,
    nick: "fred",
    welcome: "Welcome to the demo network",
    icon_url: "qrc:/demo/omarchy-icon.png",
    channels: OMARCHY_CHANNELS,
    directs: OMARCHY_DIRECTS,
    directory: OMARCHY_DIRECTORY,
};

// ---- oftc ----------------------------------------------------------------

const OFTC_OMARCHY_LINES: &[SeedLine] = &[
    chat(
        "rio",
        "This #omarchy is the OFTC one. Different people, same name.",
        "11:02",
    ),
    chat(
        "oak",
        "Good. If the sidebar mixed them, we would already be lost.",
        "11:04",
    ),
];

const OFTC_LAB_LINES: &[SeedLine] = &[
    // The one seeded transcript link. It lives on the secondary network so the
    // omarchy Status link sheet keeps only URLs a test injects.
    chat(
        "ness",
        "Package build is green: https://ci.example.com/omairc/42",
        "10:18",
    ),
    chat(
        "rio",
        "Leave the log in #build if it fails after sunset.",
        "10:21",
    ),
];

const OFTC_BUILD_LINES: &[SeedLine] = &[
    chat(
        "ness",
        "Nightly failed on missing qt6keychain. Looking.",
        "09:05",
    ),
    chat("oak", "Patched. Waiting on the next image.", "09:12"),
    chat("remy", "Image is cooking.", "09:13"),
];

const OFTC_RIO_DIRECT_LINES: &[SeedLine] =
    &[chat("rio", "Ping me on OFTC, not Freenode.", "11:40")];

const OFTC_OMARCHY_MEMBERS: &[&str] = &["rio", "ness", "oak", "pip"];
const OFTC_LAB_MEMBERS: &[&str] = &["rio", "ness", "oak", "pip", "jules", "remy"];
const OFTC_BUILD_MEMBERS: &[&str] = &["ness", "oak", "rio"];
const OFTC_AWAY_PIP: &[&str] = &["pip"];

const OFTC_OMARCHY_STATUSES: &[RankPair] = &[
    rank("rio", "on #lab"),
    rank("ness", "watching CI"),
    rank("oak", "building Omairc"),
];

const OFTC_CHANNELS: &[SeedChannel] = &[
    SeedChannel {
        name: "#omarchy",
        topic: "A different #omarchy, hosted on OFTC.",
        members: OFTC_OMARCHY_MEMBERS,
        late_join: &[],
        away: OFTC_AWAY_PIP,
        ranks: &[],
        statuses: OFTC_OMARCHY_STATUSES,
        lines: OFTC_OMARCHY_LINES,
        mark_read: true,
    },
    SeedChannel {
        name: "#lab",
        topic: "Build lab for packaging and CI.",
        members: OFTC_LAB_MEMBERS,
        late_join: &[],
        away: OFTC_AWAY_PIP,
        ranks: &[],
        statuses: &[],
        lines: OFTC_LAB_LINES,
        mark_read: true,
    },
    SeedChannel {
        name: "#build",
        topic: "Nightly builds and failing tests.",
        members: OFTC_BUILD_MEMBERS,
        late_join: &[],
        away: &[],
        ranks: &[],
        statuses: &[],
        lines: OFTC_BUILD_LINES,
        mark_read: false,
    },
];

const OFTC_DIRECTS: &[SeedDirect] = &[SeedDirect {
    nick: "rio",
    lines: OFTC_RIO_DIRECT_LINES,
    mark_read: true,
    typing: false,
}];

const OFTC_DIRECTORY: &[SeedListed] = &[SeedListed {
    name: "#debian",
    users: 28,
    topic: "Debian GNU/Linux.",
}];

/// The secondary demo network (`oftcWorld()`). It advertises no icon.
pub static OFTC_WORLD: SeedNetwork = SeedNetwork {
    network_id: OFTC_NETWORK_ID,
    nick: "oak",
    welcome: "Welcome to the demo OFTC network",
    icon_url: "",
    channels: OFTC_CHANNELS,
    directs: OFTC_DIRECTS,
    directory: OFTC_DIRECTORY,
};

/// The seeded network with `network_id`, or `None` for an unknown id.
pub fn world_for(network_id: &str) -> Option<&'static SeedNetwork> {
    match network_id {
        OMARCHY_NETWORK_ID => Some(&OMARCHY_WORLD),
        OFTC_NETWORK_ID => Some(&OFTC_WORLD),
        _ => None,
    }
}

/// The seeded channel `name` on `network`, matched case-insensitively.
pub fn channel_named<'a>(network: &'a SeedNetwork, name: &str) -> Option<&'a SeedChannel> {
    network
        .channels
        .iter()
        .find(|channel| channel.name.eq_ignore_ascii_case(name))
}
