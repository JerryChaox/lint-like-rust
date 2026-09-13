#!/usr/bin/env python3
"""Static candidate sampling, with no target imports or llr-based filtering.

Inventory only: entries require independent suitability review before admission.
"""
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import random


def inventory(source_root, manifest, seed=20260913, limit=12):
    excluded = {case['source']['path'] for case in manifest['cases']}
    # These caller files were already used to guide frontend implementation.
    excluded.add('apps/render-service/render_service/engine.py')
    candidates = []
    parse_failures = []
    paths=[]
    for directory, dirs, names in os.walk(source_root/'apps'):
        dirs[:] = sorted(d for d in dirs if not d.startswith('.') and d not in {'node_modules','__pycache__','vendor','deprecated_legacy'})
        paths.extend(Path(directory)/name for name in names if name.endswith('.py'))
    for path in sorted(paths):
        relative = path.relative_to(source_root).as_posix()
        if relative in excluded or any(part in {'.venv','node_modules','__pycache__','vendor'} for part in path.parts):
            continue
        data = path.read_bytes()
        try:
            tree = ast.parse(data, filename=relative)
        except (SyntaxError, UnicodeError) as exc:
            parse_failures.append({'path': relative, 'error': type(exc).__name__})
            continue
        lines = data.splitlines(keepends=True)
        for function in ast.walk(tree):
            if not isinstance(function, (ast.FunctionDef, ast.AsyncFunctionDef)):
                continue
            if function.end_lineno-function.lineno > 80:
                continue
            # Keep nested definitions separate; this inventory deliberately admits
            # only functions without nested function/class bodies.
            if any(isinstance(n, (ast.FunctionDef,ast.AsyncFunctionDef,ast.ClassDef))
                   for statement in function.body for n in ast.walk(statement)):
                continue
            sites = []
            for n in ast.walk(function):
                if not isinstance(n, ast.With):
                    continue
                for item in n.items:
                    call = item.context_expr
                    if (isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                            and call.func.id == 'open' and isinstance(item.optional_vars, ast.Name)):
                        sites.append({'line': n.lineno, 'receiver': item.optional_vars.id})
            if not sites:
                continue
            excerpt = b''.join(lines[function.lineno-1:function.end_lineno])
            candidates.append({'path':relative, 'function':function.name,
                'line_start':function.lineno, 'line_end':function.end_lineno,
                'file_sha256':hashlib.sha256(data).hexdigest(),
                'excerpt_sha256':hashlib.sha256(excerpt).hexdigest(),
                'resource_sites':sites, 'has_exception_handler':any(isinstance(n,ast.Try) for n in ast.walk(function)),
                'has_loop':any(isinstance(n,(ast.For,ast.While)) for n in ast.walk(function)),
                'status':'candidate_not_admitted',
                'unproven':['builtin open identity','business contract','independence from development','mutation validity']})
    random.Random(seed).shuffle(candidates)
    selected=[]
    modules=set()
    for item in candidates:
        if item['path'] in modules:
            continue
        modules.add(item['path'])
        selected.append(item)
        if len(selected)==limit:
            break
    return {'schema':'llr.repair-candidates/1','seed':seed,
            'selection':'AST with open(name) syntax; seeded shuffle; at most one function per module; no llr results used',
            'excluded_development_files':sorted(excluded), 'eligible_function_count':len(candidates),
            'requested_candidates':limit,'candidates':selected,'parse_failures':parse_failures,
            'admitted_tasks':0,'model_attempts':0}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1]
    manifest=json.loads((repo/'tests/corpus_v2/manifest.json').read_text())
    result=inventory(args.source_root,manifest)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x') as output:
        json.dump(result,output,indent=2);output.write('\n')
    print(json.dumps({k:result[k] for k in ['eligible_function_count','admitted_tasks','model_attempts']}))
    print('selected_candidates:',len(result['candidates']))


if __name__=='__main__':
    main()
