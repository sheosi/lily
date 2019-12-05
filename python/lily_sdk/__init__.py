action_classes = {}

def action(name):
	def inner_deco(cls):
		action_classes[name] = cls
		return cls

	return inner_deco