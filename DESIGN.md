---
name: Natural Skin
description: A shared native desktop photo workbench
colors:
  selection-light: "#90d1ff"
  selection-dark: "#005c80"
  canvas-charcoal: "#232527"
  panel-light: "#f8f8f8"
  panel-dark: "#1b1b1b"
  text-light: "#505050"
  text-dark: "#8c8c8c"
  control-light: "#e6e6e6"
  control-dark: "#323232"
  error-light: "#af2028"
  error-dark: "#ff9696"
typography:
  title:
    fontFamily: "Ubuntu Light, sans-serif"
    fontSize: "20px"
  heading:
    fontFamily: "Ubuntu Light, sans-serif"
    fontSize: "18px"
  body:
    fontFamily: "Ubuntu Light, sans-serif"
    fontSize: "13px"
  small:
    fontFamily: "Ubuntu Light, sans-serif"
    fontSize: "9px"
rounded:
  control: "6px"
spacing:
  related: "8px"
  inspector-inset: "12px"
  canvas-inset: "16px"
  section: "20px"
components:
  button:
    rounded: "{rounded.control}"
    typography: "{typography.body}"
    padding: "7px 12px"
  comparison-light:
    backgroundColor: "{colors.selection-light}"
    rounded: "{rounded.control}"
    typography: "{typography.body}"
  comparison-dark:
    backgroundColor: "{colors.selection-dark}"
    rounded: "{rounded.control}"
    typography: "{typography.body}"
  inspector:
    width: "300px"
    padding: "{spacing.inspector-inset}"
  image-surround:
    backgroundColor: "{colors.canvas-charcoal}"
    padding: "{spacing.canvas-inset}"
  task-status:
    height: "48px"
---

# Design System: Natural Skin

## Overview

**Creative North Star: "The Photo Workbench"**

A restrained desktop photo workbench in Operate mode. The image carries the visual weight; a fixed inspector, file actions and persistent task status support adjustment without competing with it.

The shared Windows/macOS surface follows egui’s system light/dark preference. Neutral panels, familiar controls and a constant charcoal image surround keep comparison legible. This record describes the implemented native surface, not a web theme.

**Key Characteristics:**

- Image-led workspace with a separate scrolling inspector.
- Toolkit-native states and file dialogs.
- Visible comparison, mask and task state.

Evidence: `native/src/gui.rs`, pinned egui revision `6d518fc`, and macOS empty, loaded and mask captures under `test-output/native-feasibility/gui-review/`. Windows shares the implementation; its rendered appearance has not been certified. Frontmatter CSS-compatible lengths represent egui logical points, not physical display pixels. The sidecar's HTML/CSS previews illustrate native patterns; the Rust implementation remains authoritative.

## Colors

### Primary

Selection blue is inherited from egui, with separate light and dark tokens. It marks the active Original/Retouched/Skin mask choice; file buttons remain neutral. This is the toolkit accent, not a custom brand or OS accent-color integration.

### Neutral

Canvas Charcoal is constant in both themes. Panel, text and control pairs follow egui's light/dark styles. Error text uses explicit theme-specific foregrounds: deep red in light mode and pale red in dark mode for body-text contrast. Hover, focus, active and disabled colors remain inherited from the pinned toolkit rather than an independent application palette.

**The Neutral Surround Rule.** Keep the image surround charcoal in both themes; use the toolkit selection accent for the active comparison.

## Typography

Use egui's bundled proportional family, Ubuntu Light, with toolkit glyph fallback. The title is the sole explicit size override; heading, body/button and small roles inherit the pinned toolkit defaults. Strong labels use egui emphasis rather than a separately specified font weight. Line metrics follow the toolkit.

The title identifies the app; headings identify the empty state and inspector; body serves controls and task state; small serves file metadata and preview/colour notes. There is no separate display face.

## Layout

The top file bar and bottom task bar frame a flexible left image area and fixed right inspector. The initial window is (1120 × 760 pt), with a minimum of (880 × 620 pt); no mobile composition or web breakpoints exist. The inspector width is defined by its component token; its explicit inner inset uses the inspector spacing token, in addition to toolkit panel framing. Controls wrap and scroll vertically.

Related controls use the related spacing token, with section spacing before Skin range, Skin colour, reset and help. The canvas uses its own larger inset and fits the image proportionally. The task-status token is a minimum height; error and saved-path lines can grow it. Button padding is vertical then horizontal.

**The Stable Workspace Rule.** Keep file actions and task status visible while the inspector scrolls independently.

## Elevation & Depth

The editing surface is flat: panel separators and a dark image surround establish regions. No custom shadow or decorative elevation is introduced. Tooltips and native file dialogs keep their toolkit/platform behavior.

**The Flat Workspace Rule.** Use panel boundaries and tonal contrast for the editing surface; retain toolkit tooltip and native dialog treatments.

## Shapes

Controls retain egui's gently rounded shape. The image surround is rectangular. Slider handles, checkboxes, outlines and clipping remain toolkit primitives; do not infer a custom corner system from the window's platform chrome.

## Components

### Buttons

Neutral, familiar file and reset actions use the button token. Hover changes fill/stroke, focus uses the toolkit active treatment, and unavailable controls use the toolkit disabled alpha. Open Image appears in the top bar and empty state. Export becomes available after loading and is disabled while exporting. Native file dialogs handle file choice.

### Inputs / Fields

Text labels precede each slider and are programmatically associated with it. Numeric values accompany slider tracks. Skin only and Show skin mask are explicit checkboxes. Inspector controls are unavailable until ready and during export. Radius is labelled in source-image pixels. No standalone text-entry field is present beyond toolkit numeric controls. Inspector notes identify fit-preview approximation, full-resolution export and the current sRGB workflow; they do not certify full colour management.

### Navigation

Original and Retouched are adjacent selectable controls above the canvas. Show skin mask changes the result label to Skin mask. Text and selected fill communicate the choice together; this is local comparison, not application-wide navigation.

### Image workspace

A fitted image sits within Canvas Charcoal. The empty state describes supported files and offers Open Image. The inspector scrolls without moving the image or comparison controls.

### Task status

A bottom status region shows a spinner and elapsed seconds while busy, duration/cache detail after preview, and saved path after export. Failure text uses error foreground; cancellation and finishing-file-write states remain textual. Progress is indeterminate where the backend has no reliable total. A cancellation request during the final write may report Finishing file write. No ETA is shown.

## Do's and Don'ts

### Do:

- **Do** retain the shared desktop layout and system light/dark behavior.
- **Do** keep labels attached to sliders and expose numeric adjustment values.
- **Do** distinguish Original, Retouched and Skin mask with text as well as selection state.

### Don't:

- **Don't** style disabled adjustments as available before an image is ready or during export.
- **Don't** use the image surround as a decorative accent surface.

System-font fallback is enabled for Unicode glyphs missing from the bundled fonts, including Chinese image filenames. It does not replace the primary bundled Latin typeface.

Experimental handoff mode adds Apply to PhotoCraft to the existing file toolbar and Stop Listening to the task area. Closing the resident window hides it; background logic restores it on a new capture. Normal standalone mode retains ordinary close behavior.
