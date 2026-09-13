class Worker:
    def __init__(self, f):
        self.finish(f)
    def finish(self, f):
        f.close()
