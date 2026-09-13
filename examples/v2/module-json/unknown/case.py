import json
FILE_NAME = 'settings.json'
def decode(stream):
    return json.load(stream, object_hook=custom)
def run():
    f = open(FILE_NAME)
    with f:
        return decode(f)
