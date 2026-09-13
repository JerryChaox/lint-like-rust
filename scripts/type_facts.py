"""TSP 0.4.1 boundary. Candidates are nominal evidence, never effect proofs."""
import hashlib


def source_hash(source):
    return hashlib.sha256(source.encode('utf-8')).hexdigest()


def byte_offset(source, position):
    line, column = position['line'], position['character']
    if type(line) is not int or type(column) is not int or min(line, column) < 0:
        raise ValueError('invalid_position')
    lines = source.splitlines(keepends=True)
    if source.endswith(('\n', '\r')) or not lines:
        lines.append('')
    if line >= len(lines):
        raise ValueError('invalid_position')
    text = lines[line].rstrip('\r\n')
    units = 0
    offset = sum(len(part.encode('utf-8')) for part in lines[:line])
    for char in text:
        if units == column:
            return offset
        units += len(char.encode('utf-16-le')) // 2
        offset += len(char.encode('utf-8'))
        if units > column:
            raise ValueError('split_surrogate_pair')
    if units != column:
        raise ValueError('invalid_position')
    return offset


def normalize(response, *, source, query, binding, expected_binding):
    """Caller binds a response to its request snapshot and immutable document.

    Fingerprints identify the provider, stubs and configuration. Equality checks
    prevent reuse; fingerprints do not certify provider behavior or exact dispatch.
    """
    result = {'status': 'unknown', 'candidates': [], 'reasons': [],
              'binding': dict(binding), 'dispatch': 'nominal_candidates_only'}
    required = {'snapshot', 'document_sha256', 'provider_sha256', 'stubs_sha256',
                'configuration_sha256', 'uri', 'protocol', 'query_range'}
    if (not required.issubset(binding) or binding != expected_binding
            or any(binding[k] is None or binding[k] == '' for k in required)
            or binding['protocol'] != '0.4.1'
            or binding['document_sha256'] != source_hash(source)
            or query['uri'] != binding['uri']
            or query['range'] != binding['query_range']):
        result['reasons'] = ['binding_mismatch']
        return result
    try:
        start = byte_offset(source, query['range']['start'])
        end = byte_offset(source, query['range']['end'])
        if start > end:
            raise ValueError('reversed_range')
        result['span'] = {'start': start, 'end': end}
    except (KeyError, TypeError, ValueError) as exc:
        result['reasons'] = [str(exc)]
        return result
    if 'error' in response:
        result['reasons'] = ['stale_snapshot' if response['error'].get('code') == -32802
                             else 'provider_error']
        return result
    root = response.get('result')
    ids = {}
    conflict = False

    def index(value):
        nonlocal conflict
        if isinstance(value, dict):
            if 'id' in value and 'kind' in value and value['kind'] != 9:
                ident = value['id']
                if ident in ids and ids[ident] != value:
                    conflict = True
                ids[ident] = value
            for child in value.values():
                index(child)
        elif isinstance(value, list):
            for child in value:
                index(child)

    # IDs are scoped to THIS response graph, never shared across requests.
    index(root)
    if conflict:
        result['reasons'] = ['conflicting_type_id']
        return result

    def visit(value, active):
        if not isinstance(value, dict):
            result['reasons'].append('missing_type')
            return
        if value.get('kind') == 9:
            ident = value.get('typeReferenceId')
            if ident in active or ident not in ids:
                result['reasons'].append('unresolved_type_reference')
            else:
                visit(ids[ident], active | {ident})
        elif value.get('kind') == 7:
            items = value.get('overloads', [])
            if not items:
                result['reasons'].append('empty_overload')
            for item in items:
                visit(item, active)
        elif value.get('kind') == 2:
            declaration = value.get('declaration', {})
            node = declaration.get('node', {})
            if not node.get('uri') or not node.get('range') or not declaration.get('name'):
                result['reasons'].append('missing_declaration')
            else:
                candidate = {'name': declaration['name'], 'uri': node['uri'],
                             'range_utf16': node['range']}
                if candidate not in result['candidates']:
                    result['candidates'].append(candidate)
        else:
            result['reasons'].append('unsupported_or_unknown_type')

    visit(root, set())
    if not result['reasons'] and result['candidates']:
        result['status'] = 'candidates'
    return result
