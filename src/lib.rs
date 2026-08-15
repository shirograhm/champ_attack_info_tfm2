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

const MAX_DEPTH: usize = 16;

fn join(path: &str, child: &str) -> String {
    if path.is_empty() {
        child.to_string()
    } else {
        format!("{path}.{child}")
    }
}

/// Depth-first search for node by id
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

// The container where the skill cards sit
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

const SEARCH_INTERVAL_FRAMES: u32 = 30;

#[derive(Default)]
struct State {
    screen: Option<Screen>,
    search_wait: u32,
    by_skill_text: Vec<(Pattern, String)>,
    indexed: bool,
    icons: HashMap<String, String>,
    written: String,
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

struct Pattern {
    parts: Vec<String>,
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

    fn matches(&self, text: &str) -> bool {
        if self.parts.is_empty() {
            return false;
        }
        let mut tail = text;
        for (index, part) in self.parts.iter().enumerate() {
            let first = index == 0;
            let last = index + 1 == self.parts.len();
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

// Search for icons at `mods/<mod_id>/icons/<champion>_base_attack.png`, for all mod_ids
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

            // First mod wins
            icons
                .entry(champion.to_string())
                .or_insert_with(|| format!("asset/{mod_id}/icons/{champion}_base_attack"));
        }
    }
    icons
}

impl ChampPassiveInfo {
    fn description(ctx: &StableClient<'_>, champion: &str, field: &str) -> Option<String> {
        Self::text(ctx, &format!("description.{champion}.{field}"))
    }

    fn text(ctx: &StableClient<'_>, key: &str) -> Option<String> {
        Self::text_in(ctx, TEXT_ASSET, key)
    }

    fn default_text(ctx: &StableClient<'_>, key: &str) -> Option<String> {
        Self::text_in(ctx, DEFAULTS_ASSET, key)
    }

    fn text_in(ctx: &StableClient<'_>, asset: &str, key: &str) -> Option<String> {
        let key = format!("{asset}?{key}");
        ctx.i18n(&key)
            .or_else(|| ctx.i18n(&format!("#{key}")))
            .filter(|text| !text.is_empty())
    }

    fn attack(ctx: &StableClient<'_>, champion: &str) -> Option<String> {
        Self::description(ctx, champion, "attack")
    }

    // Check for custom skill name on basic attacks at `skill_name.<champion>.attack`
    fn attack_name(ctx: &StableClient<'_>, champion: &str) -> String {
        Self::text(ctx, &format!("skill_name.{champion}.attack"))
            .or_else(|| Self::default_text(ctx, "attack_name"))
            .or_else(|| Self::text(ctx, "stat.attack"))
            .unwrap_or_default()
    }

    fn build_index(state: &mut State, ctx: &StableClient<'_>) {
        if state.indexed {
            return;
        }
        let roster = ctx.champion_names();
        if roster.is_empty() {
            return;
        }
        state.indexed = true;
        state.icons = icon_index();
        state.by_skill_text = roster
            .into_iter()
            .filter_map(|champion| {
                let skill = Self::description(ctx, &champion, "skill")?;
                Some((Pattern::new(&strip_markup(&skill)), champion))
            })
            .collect();
    }

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
    fn on_init(&self, ctx: &mut StableClient<'_>) {
        if let Ok(mut state) = self.state.lock() {
            Self::build_index(&mut state, ctx);
        }
    }

    fn post_update(&self, ctx: &mut StableClient<'_>, _dt_micros: u64) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        let Some(screen) = state.screen.take() else {
            match state.search_wait.checked_sub(1) {
                Some(remaining) => state.search_wait = remaining,
                None => {
                    state.search_wait = SEARCH_INTERVAL_FRAMES;
                    state.screen = Screen::discover(ctx);
                }
            }
            return;
        };
        if !ctx.ui_exists(&screen.node(CARD_DESC)) {
            state.written.clear();
            state.selection.clear();
            state.screen = Some(screen);
            return;
        }

        Self::build_index(&mut state, ctx);

        let shown = ctx
            .ui_text(&screen.node(PROBE))
            .map(|text| strip_markup(&text))
            .unwrap_or_default();
        let found = Self::selected(&state, &shown);
        let attack = found
            .as_ref()
            .and_then(|champion| Self::attack(ctx, champion));

        let desc = attack
            .or_else(|| Self::default_text(ctx, "attack_desc"))
            .unwrap_or_default();
        if state.written != desc {
            ctx.ui_set_text(&screen.node(CARD_DESC), &desc);
            state.written = desc;
        }

        if state.selection != shown {
            let champion = found.unwrap_or_default();
            let icon = state.icons.get(&champion).cloned().unwrap_or_default();
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
