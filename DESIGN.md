---
name: Natural Skin
description: A native PhotoCraft companion workbench
colors:
  accent: "#378ef0"
  canvas: "#282828"
  panel-dark: "#323232"
  panel-gray: "#535353"
  panel-light: "#f0f0f0"
  field-dark: "#242424"
  field-gray: "#454545"
  field-light: "#ffffff"
  text-dark: "#dedede"
  text-gray: "#eeeeee"
  text-light: "#1e1e1e"
  secondary-dark: "#bcbcbc"
  secondary-gray: "#d4d4d4"
  secondary-light: "#505050"
  selection-dark: "#2d4c70"
  selection-light: "#cde0f7"
typography:
  title:
    fontFamily: "Inter, system CJK fallback, sans-serif"
    fontSize: "16px"
  heading:
    fontFamily: "Inter, system CJK fallback, sans-serif"
    fontSize: "15px"
  body:
    fontFamily: "Inter, system CJK fallback, sans-serif"
    fontSize: "13px"
  small:
    fontFamily: "Inter, system CJK fallback, sans-serif"
    fontSize: "11px"
rounded:
  control: "4px"
spacing:
  related: "6px"
  horizontal: "8px"
  inspector-inset: "12px"
  canvas-inset: "16px"
  section: "16px"
components:
  inspector:
    width: "300px"
    padding: "{spacing.inspector-inset}"
  task-status:
    height: "48px"
---

# Natural Skin design system

## Direction and evidence

Operate mode: a restrained photo workbench that feels at home beside PhotoCraft. The image remains the largest region. The palette, field treatment, blue accent, flat panel strips and compact controls follow PhotoCraft's Pro family rather than egui's default appearance. The plugin owns its preferences independently of the editor.

Reference: PhotoCraft `docs/ui-design.md` and `crates/ui-egui/src/theme.rs`, inspected at local revision `7698a0c104d040378de00d43ba6759aaabdfaaf1`, plus its rendered macOS interface. The implementation is authoritative in `native/src/appearance.rs` and `native/src/gui.rs`. These are native egui logical points, not CSS pixels. Inter Regular is bundled under SIL OFL 1.1; system font fallback supplies Chinese glyphs. Font attribution and license are retained in `native/assets/` and `THIRD_PARTY_NOTICES.md`.

## Appearance

Dark is the initial appearance, matching PhotoCraft Pro's charcoal panels and darker fields. Gray uses Pro Medium's medium-gray panels and fields. Light uses Pro Light's pale panels and white inputs. Follow system selects the plugin's Dark or Light palette and updates with the OS. Every choice takes effect immediately and persists; the plugin does not read or alter PhotoCraft settings.

All palettes retain the charcoal image surround. Blue marks focus, selection and slider fill. Primary Apply uses a deeper blue with white text. Open, Export, reset and connection actions remain neutral. Explicit text and secondary roles maintain readable contrast; unavailable controls retain toolkit disabled treatment. Error foregrounds are pale red on dark surfaces and deep red on light surfaces.

## Structure

The top bar keeps file actions, connection access and Quit visible. A separate compact row exposes Language and Appearance on both the image and connection screens, including during processing. Image metadata and the original layer/file name stay visible and unmodified.

The right inspector is fixed at 300 points and scrolls independently. Flat strips group Skin retouching, Skin range and Skin colour. Each adjustment puts its label, slider and editable number on one row. Labels keep their accessible association and reveal their full text on hover if constrained. Notes and display-profile controls follow the main adjustments.

Original/Retouched (or Skin mask), Fit and 1:1 detail stay above the canvas. The image fits proportionally; detail is a physical-pixel crop with pointer and keyboard panning. The bottom task region stays visible during scrolling. Progress reserves the same height in idle, indeterminate and numeric states so the detail crop does not oscillate when progress appears.

Default window: 1120 × 760 points. Minimum client area: 880 × 620. Toolbars wrap; the inspector and connection setup scroll. No mobile composition ships.

## Language and copy

Language offers 简体中文 and English, using native names so the control remains discoverable in either language. First launch chooses Chinese for a Chinese system locale and English otherwise. Explicit selection is saved in Natural Skin's version-1 settings alongside appearance, connection and recovery state; missing legacy fields use defaults. Corrupt settings are preserved and a failed save is visible.

Translate source keys at render time using `native/src/locales/zh-CN.tsv`: static controls, tooltips, setup instructions, worker stages, cached-preview timing, export timing, colour notes, image metadata and known errors. Already displayed messages change with the language. Never translate layer names, paths, filenames, CLI/protocol identifiers or the installed Natural Skin Send action name. Application-owned macOS menu titles also update immediately, preserving native keyboard actions. OS-owned dialog buttons and Services content follow the OS language.

Unknown third-party/OS failures show a localized recovery message. Translated errors and unknown external failures retain the original diagnostic in a collapsed Technical details disclosure. This content is diagnostic data rather than an untranslated application label.

## Interaction and verification

Keep source pixels immutable, export to a new path, disable adjustments during export, and retain cancellation semantics. Closing a connected window minimizes it while receiving continues; Quit exits after any committed write. Do not invent ETA, HDR support or monitor calibration guarantees.

Automated checks cover settings migration and recovery, immediate language changes across 880 × 620 and 1120 × 760 layouts in all four appearance choices, preservation of image/parameters, and restored preferences after restart. macOS native inspection covers Chinese/English, loaded and connection screens and theme changes. Windows shares the code and receives a cross-target compilation check; native Windows runtime appearance remains unverified. Existing `.impeccable/design.json` is an older preview index and is not the current visual authority.
