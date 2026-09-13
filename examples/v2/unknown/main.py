from pathlib import Path
from resources import release
from external import opaque

def run():
    file = Path("example.txt").open("rb")
    alias = file
    opaque(file)
    alias.read()
