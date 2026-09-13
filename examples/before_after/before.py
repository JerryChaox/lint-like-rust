import os


def ownership():
    fd = os.open("example.txt", 0)
    stream = os.fdopen(fd, "rb")
    os.read(fd, 10)
    stream.close()


def borrowing():
    items = {"a": 1, "b": 2}
    for key in items:
        items.pop(key)


def lifetime():
    stream = open("example.txt")
    alias = stream
    stream.close()
    alias.read()
