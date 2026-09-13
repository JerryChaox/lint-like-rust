import json
def run():
    f = open('config')
    alias = f
    with f:
        try:
            return json.load(f)
        except BaseException:
            return alias.read()
