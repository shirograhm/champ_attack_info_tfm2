"""Regenerates ui/layout/champion_info.ui from the game's own copy.

Overriding a 2000-line base layout means the override goes stale every time
the game touches that screen. Rather than hand-maintain the copy, this rebuilds
it: point it at the current base_unpacked and it re-applies the same edits.

    python tools/build_layout.py [--game-dir "D:/.../Teamfight Manager2"]

The edits:
  1. A fourth card, `#passive`, cloned from `#skill1` and placed first. It
     keeps the skill card's icon, rank chip and name slots — but not its
     cooldown chip, which a basic attack has nothing to put in; the extension
     fills the rest, since the game only ever populates the three cards it
     knows.
  2. All four cards re-spaced across the same 1600px row, and their bodies
     narrowed to match. The description keeps the base game's font.
  3. Every card's header re-ordered to icon / rank / name / cooldown, from the
     base game's icon / rank / cooldown / name.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_GAME_DIR = Path("D:/SteamLibrary/steamapps/common/Teamfight Manager2")
LAYOUT_RELATIVE = Path("mods/base_unpacked/ui/layout/champion_info.ui")
OUT = ROOT / "ui" / "layout" / "champion_info.ui"

# The row is 1600px wide. Four cards of 391 with three 12px gaps fills it
# exactly; the base used three cards of ~527.
CARD_WIDTH = 391
CARD_XS = {"passive": 0, "skill1": 403, "skill2": 806, "ult": 1209}
# Card width minus its 10px left/right padding.
BODY_WIDTH = CARD_WIDTH - 20

# The header row, left to right: icon, rank chip, name, cooldown. The base game
# runs it icon/rank/cooldown/name, with the name last and widest; this puts the
# cooldown on the right edge instead and gives the name the middle. Only the two
# that move are written — the icon sits at 0 and the rank chip at 50 either way.
GAP = 10
COOLTIME_WIDTH = 78
COOLTIME_X = BODY_WIDTH - COOLTIME_WIDTH
NAME_X = 104
NAME_WIDTH = COOLTIME_X - GAP - NAME_X

CARD_INDENT = " " * 6


def extract_card(source: str, name: str) -> str:
    """Pulls one `#name:color { ... }` card block out at its own indent."""
    opener = f"{CARD_INDENT}#{name}:color {{\n"
    start = source.index(opener)
    closer = f"\n{CARD_INDENT}}}\n"
    end = source.index(closer, start) + len(closer)
    return source[start:end]


def child_span(block: str, header: str) -> tuple[int, int]:
    """Where one nested `#id:kind {` child of `block` starts and ends.

    By brace matching rather than by indent, because `#cooltime` has children of
    its own and a plain search for the next `}` would stop inside them.
    """
    start = block.index(header)
    depth = 0
    for index in range(block.index("{", start), len(block)):
        if block[index] == "{":
            depth += 1
        elif block[index] == "}":
            depth -= 1
            if depth == 0:
                return start, index + 1
    raise ValueError(f"unterminated block: {header}")


def place(block: str, header: str, x: int, width: int) -> str:
    """Moves and resizes one nested child, leaving the rest of it alone.

    The first `x`/`width` inside the child are its own — anything nested comes
    after them — so this is a `count=1` substitution over that span.
    """
    start, end = child_span(block, header)
    child = block[start:end]
    child = re.sub(r"x: \d+px;", f"x: {x}px;", child, count=1)
    child = re.sub(r"width: \d+px;", f"width: {width}px;", child, count=1)
    return block[:start] + child + block[end:]


def drop_child(block: str, header: str) -> str:
    """Removes one nested child outright, along with the blank line after it."""
    start, end = child_span(block, header)
    # Back up over the child's own indent so the line goes whole, then eat its
    # newline and the blank line that separated it from the next child. Only
    # newlines: the indent that follows belongs to the child left standing.
    start = block.rindex("\n", 0, start) + 1
    while block[end : end + 1] == "\n":
        end += 1
    return block[:start] + block[end:]


def rebuild_card(block: str, name: str) -> str:
    """Re-columns one card: new frame, new body width, new header order."""
    # Read before anything is written: cards are not all the same width in the
    # base layout (527/526/527), so the body width to look for has to come from
    # the card itself, and the first substitution below overwrites the evidence.
    old_body = int(re.search(r"width: (\d+)px;", block).group(1)) - 20

    # The card frame itself: the first width/x in the block.
    block = re.sub(r"width: \d+px;", f"width: {CARD_WIDTH}px;", block, count=1)
    block = re.sub(r"x: \d+px;", f"x: {CARD_XS[name]}px;", block, count=1)
    # Everything sized to the old body width: #data, #icon_slot, #bar, #desc.
    block = block.replace(f"width: {old_body}px;", f"width: {BODY_WIDTH}px;")

    block = place(block, "#name:label", NAME_X, NAME_WIDTH)
    block = place(block, "#cooltime:empty", COOLTIME_X, COOLTIME_WIDTH)
    return block


def build_attack_card(skill1: str) -> str:
    card = skill1.replace("#skill1:color", "#passive:color", 1)
    # Everything the extension writes needs something to fall back to when its
    # lookup comes up empty, and that is what stays in the layout. For the title
    # that is `stat.attack`, the base game's own "Attack" string, localized
    # already; for the rank chip it is the cloned card's "Lv.1".
    card = card.replace(
        'text: "#asset/base/text/champion?stat.skill";',
        'text: "#asset/base/text/champion?stat.attack";',
    )
    card = card.replace('text: "test test test lorem ipsum test";', 'text: "";')
    # A basic attack fires on a rate, not a cooldown, and the mod has nothing to
    # put in the chip — so the whole slot goes rather than sitting there empty.
    # Dropped after the re-columning, which still runs over the skill card's
    # layout: it is what gives #name the middle column the other three have.
    return drop_child(card, "#cooltime:empty")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--game-dir", type=Path, default=DEFAULT_GAME_DIR)
    args = parser.parse_args()

    base_layout = args.game_dir / LAYOUT_RELATIVE
    if not base_layout.is_file():
        raise SystemExit(f"base layout not found at {base_layout}")
    source = base_layout.read_text(encoding="utf-8")

    skill1 = extract_card(source, "skill1")
    attack = build_attack_card(rebuild_card(skill1, "passive"))

    out = source
    for name in ("skill1", "skill2", "ult"):
        card = extract_card(out, name)
        out = out.replace(card, rebuild_card(card, name), 1)

    # The new card goes in ahead of skill1.
    skill1_resized = extract_card(out, "skill1")
    out = out.replace(skill1_resized, attack + skill1_resized, 1)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(out, encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(out.splitlines())} lines)")


if __name__ == "__main__":
    main()
