# Reconciliation receipt — account-cp500 (KOBOLD.DATA.3)

EBCDIC **cp500** account family. Text DISPLAY fields are decoded through the sealed `GNURUST.15` cp500
table; **binary and packed fields are raw storage domains and are NOT text-converted** (passthrough).
Re-run is byte-stable.

| field | value |
|-------|-------|
| records | 120 |
| fields/record | 8 |
| conditions/record | 4 |
| unsupported | 0 |
| encoding | cp500 (gnucobol-3.2:ebcdic500_ascii8bit.ttbl), auto_detected=false |
| binary_fields_passthrough | true |
| packed_fields_passthrough | true |
| gnucobol-rs | 0.5.0 |
| raw_input_sha256 | `e04f3566abda6e8dda9b764149871005e4fde661ae03679a9856a182a3d6c41f` |
| layout_hash | `79a7ee7af7c94d7829876d889ddfc2f7cc0a18557c06fef5e6d266ff866b067e` |
| decode_output_sha256 | `a26763f6298c6939957cd483cc90bcea49a07579d7dd6a0ca1853897cd195a84` |

Proven: cp500 text → decoded JSON strings; LEVEL-88 evaluated on the *decoded* value (`ACTIVE`,
`CUST-GOLD`); COMP/COMP-3/COMP-5/COMP-X decoded unchanged; EBCDIC conversion never touches their bytes
(test `ebcdic_never_touches_binary_or_packed`); numeric DISPLAY under cp500 fails closed.
