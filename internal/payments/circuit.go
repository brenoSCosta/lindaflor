package payments

import (
	"sync"
	"time"
)

type breakerState int

const (
	stateClosed breakerState = iota
	stateOpen
	stateHalfOpen
)

// CircuitBreaker is a consecutive-failure breaker for outbound PSP calls.
//
// Closed: every call passes; threshold consecutive transient failures open
// it. Open: calls fail fast until cooldown elapses. Half-open: one probe is
// let through — success closes the breaker, failure re-opens it. Only
// transient failures count; a 4xx is the caller's fault, not an outage.
type CircuitBreaker struct {
	threshold uint32
	cooldown  time.Duration

	mu       sync.Mutex
	state    breakerState
	failures uint32
	until    time.Time
	probing  bool
	now      func() time.Time
}

// NewCircuitBreaker returns a closed breaker (threshold ≥ 1).
func NewCircuitBreaker(threshold uint32, cooldown time.Duration) *CircuitBreaker {
	return &CircuitBreaker{threshold: max(threshold, 1), cooldown: cooldown, now: time.Now}
}

// Allow reports whether a call may proceed now. In half-open state only
// the first caller gets the probe slot.
func (b *CircuitBreaker) Allow() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	switch b.state {
	case stateOpen:
		if !b.now().Before(b.until) {
			b.state, b.probing = stateHalfOpen, true
			return true
		}
		return false
	case stateHalfOpen:
		if b.probing {
			return false
		}
		b.probing = true
		return true
	}
	return true
}

// IsOpen is the read-only check (no probe slot consumed) used to fail
// checkout fast before creating an order.
func (b *CircuitBreaker) IsOpen() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	switch b.state {
	case stateOpen:
		return b.now().Before(b.until)
	case stateHalfOpen:
		return b.probing
	}
	return false
}

// OnSuccess closes the breaker.
func (b *CircuitBreaker) OnSuccess() {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.state, b.failures, b.probing = stateClosed, 0, false
}

// OnFailure records a transient failure.
func (b *CircuitBreaker) OnFailure() {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.state == stateClosed && b.failures+1 < b.threshold {
		b.failures++
		return
	}
	b.state, b.until, b.failures, b.probing = stateOpen, b.now().Add(b.cooldown), 0, false
}

// OnNeutral ends a call without a verdict on PSP health (e.g. a 4xx):
// releases a half-open probe slot without opening or closing.
func (b *CircuitBreaker) OnNeutral() {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.state == stateHalfOpen {
		b.probing = false
	}
}
