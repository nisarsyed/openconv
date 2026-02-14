# Code Review Interview: Section 02 - Shared Crate

**Date:** 2026-02-14

## Interview Items

### Default impl on ID types
**Decision:** Remove Default impl, use `#[allow(clippy::new_without_default)]` instead. Prevents accidental random IDs from `..Default::default()`.

## Auto-Fixes

### 1. Add doc comments to all public items
Plan specified doc comments on constants, ID types, error enum, and API structs. Adding them all.

### 2. Add user.rs roundtrip test
Only API module without tests. Adding a roundtrip serialization test for consistency.

## Let Go

- Time-sortable test sleep fragility (2ms is robust enough in practice)
- Tautological constants test (plan-specified, guards against accidental changes)
