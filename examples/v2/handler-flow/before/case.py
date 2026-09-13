import json
def run():
    f = open('config')
    alias = f
    try:
        with f:
            return json.load(f)
    except BaseException:
        return alias.read()
