def _read_contents(handle):
    return handle.read()

def _read_file(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            pass
        return _read_contents(handle)
    except Exception:
        return ""
