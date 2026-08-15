# Champ Info Basic Attack

Adds a fourth card to the Champ Info screen — the champion's basic attack —
sitting to the left of Skill 1.

The card shows a champion's `description.<champion>.attack` string — the same
key the champion's own `attack` block points its description at — titled with
`skill_name.<champion>.attack`. No base champion ships either; the base text
asset has `name`/`skill`/`skill2`/`ult` for all 64 and nothing else.

Champions that supply neither fall back to the mod's own
`text/defaults.i18n`, which describes the plain basic attack they all share:

```json
{
  "en": {
    "attack_name": "Attack",
    "attack_desc": "Deal <#ff9028ff><i#…:ad_0>100% attack damage<> as <#ff9028ff>physical damage<>."
  }
}
```

Every champion gets the card either way; the mod champions that spell their
attack out are the ones that say something different. Add a locale to that file
to translate the fallback.

## How it works

Two halves, because neither alone is enough.

**1. `ui/layout/champion_info.ui` — the card.** A full copy of the base
layout with a `#passive` card cloned from `#skill1`, placed first, and all
four cards re-spaced across the same 1600px row (391px each, 12px gaps, down
from three at ~527px). `mod.override_info` remaps
`asset/base/ui/layout/champion_info` onto it.

This geometry is fixed. The card is always there, so nothing has to move at
runtime and the extension never touches the three base cards.

Descriptions keep the base game's 15/20 font, so the longer ult text wraps to
more lines in the 371px body rather than shrinking. The attack card keeps the
skill card's header — icon, rank chip, name — minus the cooldown chip, which a
basic attack has nothing to put in. The game fills none of what remains, so the
extension writes all three. What the layout holds is only the placeholder each
one falls back to.

**2. `src/lib.rs` — the text.** The game populates `#skill1`/`#skill2`/`#ult`
by node path and knows nothing about a fourth card, so the override alone
renders an empty box. A `StableExtension` writes the description in each frame
the screen is up.

Two things the stable API does not give directly:

- *The UI path.* `ui_*` paths are dot-joined id selectors, per `UiVtableV1`'s
  own docs — **not** slash-separated. But the path a screen mounts at is not
  the path its `.ui` file implies: `champion_info` is not a node at the UI
  root, and no layout file says where it goes. So the extension does not name
  a path at all. It searches the live tree with `ui_child_names` for a node
  called `passive` and takes its parent as the row, which is both
  version-proof and cheaper to maintain than a list of guesses. The search is
  throttled to twice a second, and it runs only until the row is found the
  first time — the path is kept afterwards, closed screen or not, since a
  screen does not move between mounts within a session. From then on the
  extension's entire per-frame cost is one `ui_exists` on a path it already
  holds, and reopening the tab costs the same one call rather than another
  walk.
- *Which champion is selected.* The screen's `#champion_info/#name` label ships
  `visible: false` with empty text, so the extension reads the **skill card's**
  description off the screen and matches it against every
  `description.<id>.skill` in the text asset. Keys carry no locale segment —
  `i18n` resolves the active locale — so the comparison is against whatever
  language is showing. The card body is then `description.<id>.attack`.

  The shipped string is a template — `{Damage}`, `{Coef}` and friends are
  substituted before the screen draws them — so the comparison cannot be
  verbatim. What survives substitution is the literal text *between* the
  placeholders, so each description is reduced to those runs and the rendered
  text has to carry them in order, anchored at whichever end the template
  itself starts or ends with a literal. That is loose enough that the match
  only counts when exactly one champion claims the text; a tie is treated as no
  match rather than filling the card from the wrong champion.

  No match is not a failure state — the card falls back to the default blurb,
  the same as a champion that matched but ships no `attack` field.
- *The card title.* `skill_name.<id>.attack`, alongside the `skill1`/`skill2`/
  `ult` names the game shows itself, then `attack_name` from the defaults, then
  the base game's own `stat.attack`. That last one is the string the layout
  already ships, and unlike the defaults asset it is translated into every
  language the game ships — so it is the better answer whenever
  `defaults.i18n` has nothing for the active locale.
- *The icon.* Champions carry their basic-attack icon as
  `mods/<mod_id>/icons/<champion>_base_attack.png`. Which mod a champion came
  from is not something the API answers, and neither is "does this asset
  exist" — a missing one would render as a blank frame — so the extension
  looks on disk. The DLL is loaded by the game executable, so `current_exe`
  gives the game folder and `mods` sits beside it. Found, the icon is set as
  `asset/<mod_id>/icons/<champion>_base_attack`; not found, the icon slot goes
  `visible: false`.

  Both this table and the skill-text index are built in `on_init` — neither
  needs the screen, and `on_init` is the one hook that is not on the frame
  clock, so the champion tab never waits on them. Champion data not being
  loaded that early is the only miss, and the screen being up is proof it has
  loaded since, so that case rebuilds there.

  Title and icon are both per champion, so they are written when the selection
  changes, not per frame. The change is detected off the skill card's text
  rather than the matched id, since that text is the only thing on screen that
  moves with the selection — every unmatched champion shares the same empty id.

Nothing is reported when a step fails: a client extension cannot log —
`StableHost` is valid only inside the callback that produced it, and
`post_update` never receives one — and without a node path there is nowhere on
screen to put a message either. A failure shows up as the card keeping its
layout placeholder.

## Regenerating the layout

Overriding a 2000-line base layout means the copy goes stale whenever the game
touches that screen. Rebuild it rather than hand-merging:

```
python tools/build_layout.py [--game-dir "D:/.../Teamfight Manager2"]
```

It re-reads `mods/base_unpacked/ui/layout/champion_info.ui` and re-applies the
same edits, so a game update is a re-run and a diff.

## Building and deploying

```
.\deploy.ps1
```

Builds `champ_attack_info_tfm2.dll` and lays the mod down in the game's `mods`
folder. Close the game first — it holds the DLL open.

## Known limits

- Overriding a whole base layout conflicts with any other mod that overrides
  `champion_info`. Only one of them can win.
- `text/defaults.i18n` covers `en`, `ko`, `ja`, `de`, `vi`, `pt-BR`, `fr`,
  `es-ES`, `it`, `zh-hans` and `ru`. In the game's six other languages the
  fallback description comes out blank
  until a locale is added. The title degrades better: it falls through to the
  base game's translated `stat.attack`.
- The default blurb asserts 100% AD physical damage for every champion without
  an `attack` field. That holds for the base roster; a mod champion with an
  unusual basic attack and no `attack` description will be described wrongly
  rather than not at all.
- The selected-champion match needs the literal (non-placeholder) parts of the
  champion's skill description to appear on screen as they appear in the text
  asset. Markup is stripped from both sides, but wording that drifts from the
  shipped asset will not be found. Base champions are the standing example —
  "Leaps… dealing" on screen against "Leap… and deal" in the file — which costs
  nothing, since they have no attack text to miss. A mod champion that drifts
  the same way silently gets the default blurb instead of its own.
- A skill description that is nothing but a placeholder has no literals to
  match on, so such a champion is never identified.
- The rank chip is `attack_rank` from the defaults, the same for every champion
  — a basic attack has no rank to show. If the asset has no such key the
  layout's own `Lv.1` stands, since that reads better than an empty chip.
