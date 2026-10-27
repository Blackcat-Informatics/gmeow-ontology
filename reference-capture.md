The frozen authored packet was captured before replacing the production encoder, from base d3d7b6858303c748c3337af0a9fa95d8327cbc05 using Ciborium 0.2.2. The complete production native.rs postimage was verified byte-unchanged from that base. Only an authored helper and control were added to native.tests.rs.

The reference extends the existing tiny tables with one unqualified glyph binding and fallback, and sets latest_major to 257. It therefore contains both present and absent signature fields, Unicode text, ordered aliases/memberships and an unsigned-width boundary across all four record maps.

Command: `NEXTEST_SUCCESS_OUTPUT=immediate make nextest-synthetic RUST_PREBUILD_WORKSPACE_ARGS='-p gmeow-lang-bridge --lib' NEXTEST_FILTER='package(gmeow-lang-bridge) & test(native_transport_preserves_frozen_packet_bytes)'`.

Result: PASS, one authored case after 6 minutes 51 seconds build and 0.275 second execution; 155 unrelated cases skipped. Raw success output is in reference-capture.log. Extracted the single complete hexadecimal packet into native/fixtures/packet-v1.hex. It is 1,080 bytes; independently checked its fixed header and SHA-256 payload checksum. No corpus source, producer or fixture construction was invoked.

Removed the temporary diagnostic output and replaced it with full-packet equality against the frozen fixture before handing source ownership to the implementation author. The frozen bytes must remain unchanged. Future runtime qualification must use the replacement against this original reference; a replacement-only encode/decode round-trip is insufficient evidence.
