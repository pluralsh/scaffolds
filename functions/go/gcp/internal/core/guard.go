package core

// Guard is the result of a single safety check evaluated before an action is taken.
type Guard struct {
	Name   string `json:"name"`
	Passed bool   `json:"passed"`
	Detail string `json:"detail"`
}

// Pass returns a guard that passed.
func Pass(name, detail string) Guard {
	return Guard{Name: name, Passed: true, Detail: detail}
}

// Fail returns a guard that failed.
func Fail(name, detail string) Guard {
	return Guard{Name: name, Passed: false, Detail: detail}
}

// Check returns a guard that passed if passed is set.
func Check(name string, passed bool, detail string) Guard {
	return Guard{Name: name, Passed: passed, Detail: detail}
}

// Guards are the safety checks of one invocation, in the order they were evaluated.
type Guards []Guard

// Passed reports whether every guard passed.
func (g Guards) Passed() bool {
	for _, guard := range g {
		if !guard.Passed {
			return false
		}
	}
	return true
}
