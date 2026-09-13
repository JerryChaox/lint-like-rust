from pathlib import Path
from resources import release

def run():
    file = Path("example.txt").open("rb")
    alias = file
    release(file)
    alias.read()
