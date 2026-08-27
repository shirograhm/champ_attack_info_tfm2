Adds a fourth card to the Champ Info screen, to the left of Skill 1, showing the champion's basic attack.

The base game gives every champion three cards - Skill 1, Skill 2, and the Ultimate - and says nothing about what their basic attack actually does. Modded champions often have basic attacks that are a real part of their kit: on-hit effects, stacking debuffs, split damage. This mod gives that a card of its own.

[h1] What It Shows [/h1]
[b]Vanilla champions[/b] get a generic card: "Basic Attack - Deal 100% attack damage as physical damage."

[b]Modded champions[/b] that ship their own basic attack text get that text instead, along with a custom name and icon if the mod provides them.

The card updates as you click through the roster, and matches the styling of the three skill cards next to it.

[h1] Localization [/h1]
The default card text is translated into English, Korean, Japanese, German, Vietnamese, Brazilian Portuguese, French, Spanish, Italian, Simplified Chinese, and Russian. Custom champion text follows whatever locales that champion's own mod supports.

[h1] For Modders [/h1]
Nothing needs to be added to your mod for this to work - the card appears for every champion regardless. Everything below is optional, and only fills in the card with your own content.

[h2] Attack Description [/h2]
Add an [b]attack[/b] key alongside your existing [b]skill[/b], [b]skill2[/b], and [b]ult[/b] descriptions in your champion text asset:

[code]
"description": {
  "your_champion": {
    "attack": "Basic attacks deal 100% AD physical damage and ...",
    "skill": "...",
  }
}
[/code]

Markup tags like [b]<#ff9028ff>[/b] and the stat icons work here exactly the same as they do in skill descriptions.

[h2] Attack Name [/h2]
Add an [b]attack[/b] key under [b]skill_name[/b] for your champion:

[code]
"skill_name": {
  "your_champion": {
    "attack": "Chi Blocking",
    "skill1": "...",
  }
}
[/code]

If you leave this out, the card is titled "Basic Attack" in the player's language.

[h1] Icon Location [/h1]
[b]Put the icon at [i]<your mod folder>/icons/<champion id>_base_attack.png[/i][/b]

For example, a champion whose id is [b]ty_lee[/b] needs:

[code]
your_mod_tfm2/
  icons/
    ty_lee_base_attack.png
  text/
    champion.i18n
  mod.mod_info
[/code]

[list]
[*] [b]Champion id[/b] is the same id you key your champion under in your text asset - [b]ty_lee[/b], not "Ty Lee".
[*] [b]The suffix must be exactly[/b] [i]_base_attack.png[/i].
[*] [b]Size[/b] 64x64 PNG, to match the game's other skill icons.
[*] [b]The folder must be named[/b] [i]icons[/i], directly at the top level of your mod folder.
[/list]

That is the whole setup. You do not need to add anything to your [i]mod.override_info[/i], register the icon anywhere, or declare this mod as a dependency. The icon is found by scanning the filesystem on startup - both the game's own [i]mods[/i] folder and the Steam Workshop download folder are searched, so it works the same whether players installed your mod locally or subscribed to it on the Workshop.

A few details worth knowing:
[list]
[*] Workshop folders are named by numeric file id rather than by mod id, so the mod id is read out of your [i]mod.mod_info[/i] to build the asset path. Keep that file accurate.
[*] If two installed mods both ship an icon for the same champion id, the first one found wins.
[*] If no icon is found, the card simply renders without one - the icon and its background are hidden, and nothing breaks.
[/list]

[h1] Compatibility [/h1]
This mod overrides the champion info layout. Another mod that overrides the same layout will conflict with it - only one of the two will take effect.

[h2] Credits [/h2]
Thank you to the people in the modding discord for their help with the mod-sdk setup, documentation, and general coolness.

[Code Mod Notice]
This Workshop item contains native/executable code files. Enabling it allows code to run inside the game process. Use only mods from creators you trust.

Files: champ_attack_info_tfm2.dll

Runs on: Windows
