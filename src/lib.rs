use mod_api_stable::*;
use std::collections::HashMap;
use std::sync::Mutex;

const MOD_ID: &str = "champ_attack_info_tfm2";

const TEXT_ASSET: &str = "asset/base/text/champion";
const DEFAULTS_ASSET: &str = "asset/champ_attack_info_tfm2/text/defaults";

const CARD_DESC: &str = "passive.data.desc";
const CARD_ICON: &str = "passive.data.icon_slot.icon";
const CARD_ICON_BG: &str = "passive.data.icon_slot.icon_bg";
const CARD_NAME: &str = "passive.data.icon_slot.name";
const CARD_RANK: &str = "passive.data.icon_slot.text";
const PROBE: &str = "skill1.data.desc";
const CARD: &str = "passive";

/// How deep the search for the card goes. The screen sits an unknown number
/// of levels below the UI root — which is the whole reason for searching
/// rather than naming a path — so this only has to be "deeper than that".
const MAX_DEPTH: usize = 16;

fn join(path: &str, child: &str) -> String {
    if path.is_empty() {
        child.to_string()
    } else {
        format!("{path}.{child}")
    }
}

/// Depth-first search fornode by id
fn find_node(ctx: &StableClient<'_>, path: &str, target: &str, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    ctx.ui_child_names(path).into_iter().find_map(|child| {
        let child_path = join(path, &child);
        if child == target {
            Some(child_path)
        } else {
            find_node(ctx, &child_path, target, depth + 1)
        }
    })
}

/// The container where the skill cards sit
struct Screen {
    root: String,
}

impl Screen {
    fn node(&self, suffix: &str) -> String {
        join(&self.root, suffix)
    }

    fn discover(ctx: &StableClient<'_>) -> Option<Self> {
        let card = find_node(ctx, "", CARD, 0)?;
        let root = card.strip_suffix(CARD)?.trim_end_matches('.').to_string();
        let screen = Self { root };
        ctx.ui_exists(&screen.node(CARD_DESC)).then_some(screen)
    }
}

/// Frames between searches for the row, ~0.5s at 60fps — slow enough that the
/// tree walk is not a per-frame cost, quick enough not to show as a delay when
/// the screen opens.
const SEARCH_INTERVAL_FRAMES: u32 = 30;

/// Champions indexed per frame while the screen is not open.
///
/// The index is built ahead of time precisely so the champion screen does not
/// pay for it, so it must not cost a visible frame of its own either — a few
/// per frame finishes a full roster in a couple of seconds of menu time, which
/// is far less than the wait before anyone reaches the tab.
const INDEX_CHUNK: usize = 8;

#[derive(Default)]
struct State {
    screen: Option<Screen>,
    search_wait: u32,
    /// Skill-description pattern -> champion id. The screen never says which
    /// champion is selected, so the skill text is the identifier.
    by_skill_text: Vec<(Pattern, String)>,
    indexed: bool,
    /// Champions the index has yet to cover, popped from the back. Empty and
    /// `indexed` both mean "done"; this exists so the work can be spread over
    /// idle frames instead of landing on the one that opens the screen.
    pending: Vec<String>,
    /// Frames left before asking the game for the roster again. Only ticks
    /// before the roster first comes back non-empty — champion data is not
    /// loaded during the very first frames, and an empty answer then would
    /// otherwise be cached as the whole index.
    roster_wait: u32,
    /// Basic-attack icons by champion, from [`icon_index`]. `None` until built.
    icons: Option<HashMap<String, String>>,
    written: String,
    /// The skill text the card header was last filled for. The icon and title
    /// change on selection, not per frame.
    selection: String,
}

struct ChampPassiveInfo {
    state: Mutex<State>,
}

fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// A skill description from the text asset, as something the rendered text can
/// be checked against.
///
/// The shipped string is a template: `{Damage}`, `{Coef}` and friends are
/// substituted before the screen draws them, so comparing the two verbatim only
/// ever matches champions whose description happens to have no placeholders at
/// all. What survives substitution is the literal text *between* the
/// placeholders, so that is what is kept.
struct Pattern {
    /// The literal runs, in order, with the placeholders removed. Empty runs
    /// (a placeholder at either end, or two in a row) are dropped, so a leading
    /// or trailing one leaves the corresponding end unanchored.
    parts: Vec<String>,
    /// Whether the template began and ended with literal text, i.e. whether
    /// `parts` has to line up with the very start and end of the rendered
    /// string rather than merely appear inside it.
    anchor_start: bool,
    anchor_end: bool,
}

impl Pattern {
    fn new(template: &str) -> Self {
        let mut parts: Vec<String> = Vec::new();
        let mut literal = String::new();
        let mut depth = 0usize;
        for ch in template.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    if depth == 1 && !literal.is_empty() {
                        parts.push(std::mem::take(&mut literal));
                    }
                }
                '}' => depth = depth.saturating_sub(1),
                _ if depth == 0 => literal.push(ch),
                _ => {}
            }
        }
        let anchor_end = !literal.is_empty();
        if anchor_end {
            parts.push(literal);
        }
        Self {
            anchor_start: !template.starts_with('{'),
            anchor_end,
            parts,
        }
    }

    /// Whether `text` could be this template with its placeholders filled in.
    ///
    /// Loose by construction — the placeholder values themselves are not
    /// checked, only that the literals appear in order — which is why callers
    /// require the match to be unique across all champions.
    fn matches(&self, text: &str) -> bool {
        if self.parts.is_empty() {
            return false;
        }
        let mut tail = text;
        for (index, part) in self.parts.iter().enumerate() {
            let first = index == 0;
            let last = index + 1 == self.parts.len();
            // The closing literal has to sit at the very end when the template
            // ended with one; anything after it is a different description. A
            // template with no placeholders at all is both the opening and the
            // closing literal, so it has to be the whole string.
            if last && self.anchor_end {
                let Some(head) = tail.strip_suffix(part.as_str()) else {
                    return false;
                };
                if first && self.anchor_start && !head.is_empty() {
                    return false;
                }
                tail = "";
            } else if first && self.anchor_start {
                match tail.strip_prefix(part.as_str()) {
                    Some(rest) => tail = rest,
                    None => return false,
                }
            } else {
                match tail.find(part.as_str()) {
                    Some(at) => tail = &tail[at + part.len()..],
                    None => return false,
                }
            }
        }
        true
    }
}

/// Every basic-attack icon any mod ships, as champion id -> asset path.
///
/// Icons live at `mods/<mod_id>/icons/<champion>_base_attack.png`, and which
/// mod a champion came from is not something the stable API answers — nor is
/// "does this asset exist", so a missing file would silently render as a blank
/// box. Hence the disk lookup: the DLL is loaded by the game executable, so
/// `current_exe` is the game folder and `mods` sits beside it.
///
/// Read whole rather than per champion, because the caller wants it warm
/// before the first selection lands rather than a directory walk on the frame
/// the card is first filled. Mods are not installed while the game is running,
/// so one pass holds for the session.
fn icon_index() -> HashMap<String, String> {
    let mut icons = HashMap::new();
    let Some(mods) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("mods")))
    else {
        return icons;
    };
    for entry in std::fs::read_dir(mods).into_iter().flatten().flatten() {
        let Some(mod_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        for icon in std::fs::read_dir(entry.path().join("icons"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = icon.file_name();
            let Some(champion) = name
                .to_str()
                .and_then(|name| name.strip_suffix("_base_attack.png"))
            else {
                continue;
            };
            // First mod wins, matching the old search order: the directory scan
            // it replaced stopped at the first hit too.
            icons
                .entry(champion.to_string())
                .or_insert_with(|| format!("asset/{mod_id}/icons/{champion}_base_attack"));
        }
    }
    icons
}

impl ChampPassiveInfo {
    /// Keys carry no locale segment — `i18n` resolves the active locale — so
    /// this is `description.archer.skill`, exactly as the `.data_champion`
    /// files and the base layout spell it. The leading `#` those files use is
    /// an asset-reference marker; whether this call wants it is undocumented,
    /// so try both.
    fn description(ctx: &StableClient<'_>, champion: &str, field: &str) -> Option<String> {
        Self::text(ctx, &format!("description.{champion}.{field}"))
    }

    /// One key out of the champion text asset.
    fn text(ctx: &StableClient<'_>, key: &str) -> Option<String> {
        Self::text_in(ctx, TEXT_ASSET, key)
    }

    /// One key out of the mod's own `text/defaults.i18n`.
    fn default_text(ctx: &StableClient<'_>, key: &str) -> Option<String> {
        Self::text_in(ctx, DEFAULTS_ASSET, key)
    }

    /// One key out of a text asset. The leading `#` the data files use is an
    /// asset-reference marker; whether this call wants it is undocumented, so
    /// try both.
    fn text_in(ctx: &StableClient<'_>, asset: &str, key: &str) -> Option<String> {
        let key = format!("{asset}?{key}");
        ctx.i18n(&key)
            .or_else(|| ctx.i18n(&format!("#{key}")))
            .filter(|text| !text.is_empty())
    }

    /// The basic-attack blurb, keyed `description.<champion>.attack` — the same
    /// field the champion's own `attack` block points its description at.
    fn attack(ctx: &StableClient<'_>, champion: &str) -> Option<String> {
        Self::description(ctx, champion, "attack")
    }

    /// The card's title: the champion's own name for its basic attack, keyed
    /// `skill_name.<champion>.attack` alongside the three the game itself
    /// shows, falling back to `attack_name` from the mod's defaults.
    ///
    /// The base game's own `stat.attack` backs both up. It is the string the
    /// layout already holds, and unlike the defaults asset it is translated
    /// into every language the game ships, so it is the better answer whenever
    /// `defaults.i18n` has nothing for the active locale.
    fn attack_name(ctx: &StableClient<'_>, champion: &str) -> String {
        Self::text(ctx, &format!("skill_name.{champion}.attack"))
            .or_else(|| Self::default_text(ctx, "attack_name"))
            .or_else(|| Self::text(ctx, "stat.attack"))
            .unwrap_or_default()
    }

    /// Advances the skill-text index by up to `budget` champions, and builds
    /// the icon table on the way past.
    ///
    /// Called every frame, screen open or not: the whole point is that by the
    /// time someone clicks the champion tab this has already finished, so the
    /// first frame of the screen only has to read a probe and fill a card.
    /// `budget` is [`INDEX_CHUNK`] on those idle frames and unbounded once the
    /// screen is actually up, since a card that renders a frame late is worse
    /// than a frame that runs long.
    ///
    /// Returns whether the index is complete.
    fn index_step(state: &mut State, ctx: &StableClient<'_>, budget: usize) -> bool {
        if state.indexed {
            return true;
        }

        // The roster arrives once champion data is loaded, which is not true on
        // the first frames. Until it does, ask at the same throttled rate the
        // row search uses rather than every frame.
        if state.pending.is_empty() && state.by_skill_text.is_empty() {
            match state.roster_wait.checked_sub(1) {
                Some(remaining) => {
                    state.roster_wait = remaining;
                    return false;
                }
                None => {
                    state.roster_wait = SEARCH_INTERVAL_FRAMES;
                    state.pending = ctx.champion_names();
                    // Popped from the back, so reversing keeps the index in
                    // roster order. Nothing depends on the order — the lookup
                    // requires a unique match either way — but a shuffled index
                    // would be a confusing thing to debug from.
                    state.pending.reverse();
                    if state.pending.is_empty() {
                        return false;
                    }
                }
            }
        }

        // Disk, not the game: independent of the roster, and wanted warm for
        // the same reason. One directory walk, on whichever frame gets here
        // first.
        if state.icons.is_none() {
            state.icons = Some(icon_index());
        }

        for _ in 0..budget {
            let Some(champion) = state.pending.pop() else {
                break;
            };
            if let Some(skill) = Self::description(ctx, &champion, "skill") {
                state
                    .by_skill_text
                    .push((Pattern::new(&strip_markup(&skill)), champion));
            }
        }

        // Built once, not "once it comes back non-empty" — a champion with no
        // skill text is an answer too, and retrying the roster every frame was
        // 93 champions' worth of lookups per frame.
        state.indexed = state.pending.is_empty();
        state.indexed
    }

    /// Which champion the screen is showing, read off its skill card.
    ///
    /// Two passes, because [`Pattern::matches`] ignores whatever stood where
    /// the placeholders were: an exact hit is taken outright, and a pattern hit
    /// only counts when exactly one champion claims the text. Two champions
    /// sharing every literal of a description would otherwise be decided by
    /// index order, and filling the card from the wrong one is worse than
    /// leaving it off.
    fn selected(state: &State, shown: &str) -> Option<String> {
        if shown.is_empty() {
            return None;
        }
        let exact = state
            .by_skill_text
            .iter()
            .find(|(skill, _)| skill.anchor_start && skill.anchor_end && skill.parts == [shown]);
        if let Some((_, champion)) = exact {
            return Some(champion.clone());
        }
        let mut hits = state
            .by_skill_text
            .iter()
            .filter(|(skill, _)| skill.matches(shown));
        let (_, champion) = hits.next()?;
        hits.next().is_none().then(|| champion.clone())
    }
}

impl StableExtension for ChampPassiveInfo {
    fn post_update(&self, ctx: &mut StableClient<'_>, _dt_micros: u64) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        // Ahead of the screen check, so the roster index and the icon table are
        // built out of menu frames nobody is watching. Whoever opens the
        // champion tab then finds the work already done.
        Self::index_step(&mut state, ctx, INDEX_CHUNK);

        // Cheap: confirms the row the mod already found is still mounted.
        if !state
            .screen
            .as_ref()
            .is_some_and(|screen| ctx.ui_exists(&screen.node(CARD_DESC)))
        {
            state.screen = None;
            state.written.clear();
            state.selection.clear();

            // Expensive: searches the whole tree. Throttled, so the cost lands
            // a couple of times a second rather than every frame — the screen
            // opening is what it is waiting for, and the first failure comes
            // during the startup disclaimer, long before that.
            match state.search_wait.checked_sub(1) {
                Some(remaining) => {
                    state.search_wait = remaining;
                    return;
                }
                None => {
                    state.search_wait = SEARCH_INTERVAL_FRAMES;
                    state.screen = Screen::discover(ctx);
                    if state.screen.is_none() {
                        return;
                    }
                }
            }
        }
        let Some(screen) = state.screen.take() else {
            return;
        };

        // Normally a no-op by now. It is not when the screen opens during the
        // first seconds of a session, or when champion data only loaded once a
        // save was, so the remainder is finished outright here rather than
        // trickled out while a half-filled index picks the wrong champion.
        // The one way this still fails is champion data not being loaded at
        // all; the row is kept so the next frame retries the index rather than
        // the tree walk.
        if !Self::index_step(&mut state, ctx, usize::MAX) {
            state.screen = Some(screen);
            return;
        }

        // The skill card's text, which is both how the champion is identified
        // and — since it changes with the selection and nothing else does —
        // what tells the mod a different champion is up.
        let shown = ctx
            .ui_text(&screen.node(PROBE))
            .map(|text| strip_markup(&text))
            .unwrap_or_default();
        let found = Self::selected(&state, &shown);
        let attack = found
            .as_ref()
            .and_then(|champion| Self::attack(ctx, champion));

        // No `attack` field — a base champion, or one whose skill text did not
        // match — still gets the card, filled from the mod's own
        // `text/defaults.i18n` with the plain basic attack every champion has.
        let desc = attack
            .or_else(|| Self::default_text(ctx, "attack_desc"))
            .unwrap_or_default();
        if state.written != desc {
            ctx.ui_set_text(&screen.node(CARD_DESC), &desc);
            state.written = desc;
        }

        // The icon, the title and the rank chip are all written here — on
        // selection, since none of them change per frame. A champion whose mod
        // ships no icon hides that slot rather than showing an empty frame.
        //
        // Keyed on the skill text rather than the matched id, because that is
        // the only thing on screen that changes with the selection — the
        // unmatched champions all share the same empty id.
        if state.selection != shown {
            let champion = found.unwrap_or_default();
            let icon = state
                .icons
                .as_ref()
                .and_then(|icons| icons.get(&champion))
                .cloned()
                .unwrap_or_default();
            for node in [CARD_ICON, CARD_ICON_BG] {
                let props = format!("visible: {};", !icon.is_empty());
                ctx.ui_set_properties(&screen.node(node), &props);
            }
            if !icon.is_empty() {
                let props = format!("source: \"{icon}\";");
                ctx.ui_set_properties(&screen.node(CARD_ICON), &props);
            }

            let name = Self::attack_name(ctx, &champion);
            ctx.ui_set_text(&screen.node(CARD_NAME), &name);

            // A basic attack has no rank to show, so this is a fixed string
            // rather than anything per champion — but it is still text on
            // screen, so it comes out of the defaults asset like the rest.
            // Left alone when the asset has nothing, since the layout's own
            // "Lv.1" is a better placeholder than an empty chip.
            let rank = Self::default_text(ctx, "attack_rank").unwrap_or_default();
            if !rank.is_empty() {
                ctx.ui_set_text(&screen.node(CARD_RANK), &rank);
            }

            state.selection = shown;
        }

        state.screen = Some(screen);
    }
}

fn init(host: &StableHost) -> StableMod {
    host.log(
        LogLevel::Info,
        "champ_attack_info_tfm2: basic attack card extension registering",
    );
    let mut reg = StableMod::new(MOD_ID);
    reg.set_extension(ChampPassiveInfo {
        state: Mutex::new(State::default()),
    });
    reg
}

declare_stable_mod!(init);
