import os


def ownership():
    fd = os.open("example.txt", 0)
    with os.fdopen(fd, "rb") as stream:
        stream.read(10)


def borrowing():
    items = {"a": 1, "b": 2}
    for key in list(items):
        items.pop(key)


def lifetime():
    with open("example.txt") as stream:
        alias = stream
        alias.read()
