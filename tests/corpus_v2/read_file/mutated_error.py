

def _read_file(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            pass
        return handle.read()
    except Exception:
        return ""

