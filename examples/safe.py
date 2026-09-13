# Static-analysis example; scanning does not execute this file.
with open("example.txt") as stream:
    stream.read()

items = {"a": 1}
for key in list(items):
    items.pop(key)
