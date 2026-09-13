"""Verify corpus provenance and syntax without importing or executing fixture code."""
import argparse
import ast
import hashlib
import json
from pathlib import Path


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, help='Optionally verify against current Museon checkout')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / 'manifest.json').read_text())
    count = 0
    for case in manifest['cases']:
        source = case['source']
        excerpt = (root / source['excerpt_path']).read_bytes()
        assert digest(excerpt) == source['excerpt_sha256'], case['id']
        if args.source_root:
            raw = (args.source_root / source['path']).read_bytes()
            assert digest(raw) == source['file_sha256'], f"Source changed: {source['path']}"
            lines = raw.splitlines(keepends=True)
            assert b''.join(lines[source['line_start'] - 1:source['line_end']]) == excerpt
        original = (source['imports_scaffolding'] + '\n\n').encode() + excerpt
        assert (root / case['variants']['original']['path']).read_bytes() == original
        for name, variant in case['variants'].items():
            raw = (root / variant['path']).read_bytes()
            assert digest(raw) == variant['sha256'], (case['id'], name)
            ast.parse(raw, filename=variant['path'])
            assert variant['origin'] in {
                'exact_source_excerpt_with_import_scaffolding', 'synthetic_mutation',
                'synthetic_repair_restoring_original', 'synthetic_cross_function_refactor',
            }
            count += 1
    print(f"Verified {len(manifest['cases'])} source excerpts and {count} variants; no fixture executed.")


if __name__ == '__main__':
    main()
