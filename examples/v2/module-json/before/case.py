import json
FILE_NAME = 'settings.json'
def decode(stream):
    return json.load(stream)
def run():
    f = open(FILE_NAME)
    f.close()
    return decode(f)
