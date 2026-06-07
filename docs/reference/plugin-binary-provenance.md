# Plugin Binary Provenance

Last Modified: 2026-06-05

`plugins/axon/bin/axon` is a checked-in plugin binary used by the Axon plugin
package. Binary replacements require an explicit review note because normal text
diff review cannot show executable changes.

Current reviewed artifact:

- Path: `plugins/axon/bin/axon`
- SHA-256: `086d041c3413a1edb4832f3fd393451255110cd581eaa23b4decc199d4df28d8`
- File type: `ELF 64-bit LSB pie executable, x86-64`
- Size: `245M`
- Build ID: `e09c52429f2417f0af5d83d2ef5330d1b78583cc`
- Debug info: present
- Stripped: no - inherited artifact. Future binary replacements should be
  release-built and stripped before review unless debug symbols are explicitly
  required.

Verification command:

```bash
sha256sum plugins/axon/bin/axon
file plugins/axon/bin/axon
ls -lh plugins/axon/bin/axon
```
