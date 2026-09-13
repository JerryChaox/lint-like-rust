# Static-analysis examples only; do not execute this file.
import os


def moved_descriptor():
    fd = os.open("example.txt", 0)
    alias = fd
    stream = os.fdopen(fd, "r")
    os.read(alias, 10)
    stream.close()


def conflicting_borrows():
    items = {"a": 1}
    for key in items:
        items.pop(key)


def closed_resource():
    stream = open("example.txt")
    alias = stream
    stream.close()
    alias.read()
