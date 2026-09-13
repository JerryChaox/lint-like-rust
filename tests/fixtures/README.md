# Independent semantic cases

`tests/semantics.rs` exercises Python lowering and the shared analysis together.
Positive cases require a diagnostic; safe cases require no diagnostic but may have
explicit coverage gaps. No test treats an unmodeled operation as verified safe.

`tests/python_frontend.rs` covers the current raw-descriptor model: resolved
`os.open` creates a tracked resource; `os.read`, `os.write`, `os.fsync`, and
`os.close` check its validity. Successful `os.fdopen` with default or literal
`closefd=True` transfers the tracked descriptor, including its aliases, and
creates a usable wrapper resource. Literal `closefd=False` preserves descriptor
ownership. Resolved import aliases work; a shadowed `os` binding does not acquire
these effects merely from method spelling.

Coverage remains incomplete: every `os.fdopen` reports unmodeled exceptional
construction outcomes. Unresolved descriptor identities, dynamic `closefd`, and
unsupported argument forms retain explicit coverage gaps instead of assuming
transfer. Integer-descriptor `open(fd)` is not modeled. Descriptor construction
through `tempfile.mkstemp` is also outside this model, so Museon's
`replacement_marker.py` pattern is not fully verified by the `os.open` support.
