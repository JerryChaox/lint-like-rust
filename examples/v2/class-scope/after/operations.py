class Worker:
    def finish(self, f):
        f.close()
    def read(self, f):
        return self.contents(f)
    def contents(self, f):
        return f.read()
