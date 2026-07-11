class Handler:
    def handle(self, event):
        return event

    def validate(self, event):
        return bool(event)
