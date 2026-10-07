# Disposable VM testing

The default account for disposable installed-OS VM tests is:

| Setting | Value |
| --- | --- |
| Username | `02tester` |
| Password | `Cs@12345` |
| Sudo | Enabled; use the same password |
| Current native VM window | `02_OS - Installed VM` |

Read [`scripts/vm-test-credentials.json`](../scripts/vm-test-credentials.json) before running test automation or signing in. That fixture is the source of truth for credentials; automation should load it directly. Use the same password for login, session unlock, and sudo.

For a reused VM or older snapshot, apply the fixture's password to `02tester`, then verify authentication before testing. Do not infer the password from earlier chat messages or test artifacts.

These defaults apply to disposable OS test VMs. Production installations use the account chosen during installation, and the live image retains its separate account documented in [`README.md`](../README.md).

Keep historical captures and reports unchanged. Record the applied credentials and verification outcome in a new test record when updating an existing VM.
