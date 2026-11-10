# Natural Skin

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

Desktop only: Windows and macOS. Mobile platforms are outside this implementation.

## Stack

User confirmed Windows/macOS from the first release. Rust core and egui desktop frontend; framework selected to share processing, worker lifecycle and preview caching across both systems. No web runtime or PhotoCraft modifications.

## Users and Purpose

PhotoCraft users retouching skin who need responsive parameter previews and visible processing/completion status. The user's original request established source preservation, skin-range control and a native companion as the development direction.

## Workflow

Normal launch offers optional PhotoCraft connection setup. Enable it, import the generated action once, then send a selected ordinary pixel layer from PhotoCraft, adjust against an immutable source, compare the result/mask and Apply to open a new host document. Manual pcraft/TIFF opening and new-file export remain available. Settings persist, duplicate launches activate the existing instance, closing minimizes while connected, and Quit exits. Login startup is opt-in; completed results remain recoverable. Original-layer replacement is outside this workflow.

## Constraints and Evidence

PhotoCraft must remain unchanged. No second WASM product. The actual desktop .pcraft round trip now preserves 8/16/32-bit samples, hidden RGB, Alpha, ICC and DPI on synthetic multi-tile fixtures. File processing is capped at 36 MP, preview at 2 MP. General layer trees and wide-gamut/HDR retouching are not certified. Current development machine is macOS arm64; Windows runtime verification remains separate.

## Principles

- Keep the original immutable; always export to a new path.
- Reuse the preview basis when mixing parameters change.
- Keep expensive work off the UI thread and discard stale results.
- Show real task stages and elapsed time; do not invent ETA.

## Open Decisions

Windows runtime validation, distribution signing and broad colour-space support remain to be developed. The independent interface supports Simplified Chinese and English with immediate, persistent selection; first launch uses the system locale. PhotoCraft-style Dark, Gray and Light themes plus system appearance use the existing Pro workbench vocabulary. No user photos were supplied.

## Action handoff

A native JSON script creates a PhotoCraft action that exports the selected ordinary pixel layer to a dedicated exchange file. A resident Natural Skin receiver raises its separate window, and Apply returns a new host document through OS file opening. This was exercised on macOS with Agent Control disabled. Version 0.2.0 implements one-time connection setup and optional startup. Windows runtime verification remains unfinished. No control token or editor settings changes are required.

## Detail and colour acceptance (2026-11-10)

Fit remains approximate. Physical-pixel detail crops share the export kernel and full source context, with pointer/keyboard navigation. Exports use bounded tiles. SDR RGB matrix/TRC input profiles convert through encoded sRGB and back to original encoding, preserving unchanged/protected samples. Manual display-output ICC selection is preview-only and temporary; monitor discovery, calibrated-display/HDR and real-portrait certification remain open. Synthetic samples are used because the user has no authorized portraits available.
