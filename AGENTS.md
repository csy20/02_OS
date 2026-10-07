# Project guidance

## Disposable OS VM tests

- Read `scripts/vm-test-credentials.json` before provisioning or signing into a test VM. It is the canonical credential fixture: username `02tester`, password `Cs@12345`, sudo enabled.
- Use these credentials for disposable installed-OS VM tests. Do not invent another password or reuse an older password from chat history, screenshots, logs, or snapshots.
- When reusing a legacy test VM or snapshot, apply the canonical password to `02tester` and verify authentication before continuing.
- Keep these credentials out of production accounts and live-image/installer account defaults. The live image's separate `live` account is documented in `README.md`.
- Preserve historical test evidence; record credential changes in a new validation record instead of rewriting old results.
- See `docs/VM_TESTING.md` for the test account and visible VM details.
