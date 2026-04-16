package nebulatest

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"
)

// ── logger.go ──
// Logger is a tiny leveled wrapper used by KVCache and the API layer.
type Logger struct {
	prefix string
	out    *log.Logger
}

// NewLogger returns a Logger with the given prefix that writes to stderr.
func NewLogger(prefix string) *Logger {
	return &Logger{
		prefix: prefix,
		out:    log.New(os.Stderr, "", log.LstdFlags),
	}
}

// Debugf formats and logs a debug message.
func (l *Logger) Debugf(format string, args ...interface{}) {
	l.out.Printf("[DEBUG %s] %s", l.prefix, fmt.Sprintf(format, args...))
}

// Infof formats and logs an info message.
func (l *Logger) Infof(format string, args ...interface{}) {
	l.out.Printf("[INFO %s] %s", l.prefix, fmt.Sprintf(format, args...))
}

// Errorf formats and logs an error message.
func (l *Logger) Errorf(format string, args ...interface{}) {
	l.out.Printf("[ERROR %s] %s", l.prefix, fmt.Sprintf(format, args...))
}

// ── metrics.go ──
// Metrics tracks KVCache hit/miss counters in a goroutine-safe way.
type Metrics struct {
	hits   uint64
	misses uint64
}

// NewMetrics returns a fresh Metrics instance with zeroed counters.
func NewMetrics() *Metrics {
	return &Metrics{}
}

// RecordHit is called by KVCache.Get on a successful lookup.
func (m *Metrics) RecordHit() {
	atomic.AddUint64(&m.hits, 1)
}

// RecordMiss is called by KVCache.Get when the key is absent or expired.
func (m *Metrics) RecordMiss() {
	atomic.AddUint64(&m.misses, 1)
}

// Snapshot returns the current hit/miss counters.
func (m *Metrics) Snapshot() (hits, misses uint64) {
	return atomic.LoadUint64(&m.hits), atomic.LoadUint64(&m.misses)
}

// ── validator.go ──
// Validator enforces simple shape rules on KVCache keys and values.
type Validator struct {
	maxKeyLen   int
	maxValueLen int
}

// NewValidator constructs a Validator with the given length limits.
func NewValidator(maxKeyLen, maxValueLen int) *Validator {
	return &Validator{maxKeyLen: maxKeyLen, maxValueLen: maxValueLen}
}

// Key validates a single KVCache key.
func (v *Validator) Key(key string) error {
	if key == "" {
		return errors.New("key must not be empty")
	}
	if utf8.RuneCountInString(key) > v.maxKeyLen {
		return errors.New("key too long")
	}
	return nil
}

// KeyValue validates a key/value pair before insertion into the KVCache.
func (v *Validator) KeyValue(key, value string) error {
	if err := v.Key(key); err != nil {
		return err
	}
	if utf8.RuneCountInString(value) > v.maxValueLen {
		return errors.New("value too long")
	}
	return nil
}

// ── cache.go ──
// KVCache is an in-memory key/value store with optional TTL semantics.
// Values are kept until either Delete is called or the entry's TTL expires.
// All operations are safe for concurrent use.
type KVCache struct {
	mu      sync.RWMutex
	data    map[string]kvEntry
	maxSize int
	logger  *Logger
	metrics *Metrics
}

type kvEntry struct {
	value     string
	expiresAt time.Time // zero value means no expiry
}

// NewKVCache constructs a KVCache with the given maximum entry count and
// optional logger/metrics hooks. A maxSize of 0 means unbounded.
func NewKVCache(maxSize int, logger *Logger, metrics *Metrics) *KVCache {
	return &KVCache{
		data:    make(map[string]kvEntry),
		maxSize: maxSize,
		logger:  logger,
		metrics: metrics,
	}
}

// Get returns the value associated with key and a boolean indicating presence.
// Expired entries are treated as missing and lazily evicted on read.
func (c *KVCache) Get(key string) (string, bool) {
	c.mu.RLock()
	entry, ok := c.data[key]
	c.mu.RUnlock()
	if !ok {
		c.metrics.RecordMiss()
		return "", false
	}
	if !entry.expiresAt.IsZero() && time.Now().After(entry.expiresAt) {
		c.mu.Lock()
		delete(c.data, key)
		c.mu.Unlock()
		c.metrics.RecordMiss()
		return "", false
	}
	c.metrics.RecordHit()
	return entry.value, true
}

// Set inserts or updates an entry with no expiry.
func (c *KVCache) Set(key, value string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.maxSize > 0 && len(c.data) >= c.maxSize {
		c.evictOldest()
	}
	c.data[key] = kvEntry{value: value}
	c.logger.Debugf("KVCache.Set %q", key)
}

// SetTTL inserts or updates an entry with a time-to-live duration.
func (c *KVCache) SetTTL(key, value string, ttl time.Duration) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.maxSize > 0 && len(c.data) >= c.maxSize {
		c.evictOldest()
	}
	c.data[key] = kvEntry{value: value, expiresAt: time.Now().Add(ttl)}
	c.logger.Debugf("KVCache.SetTTL %q (%s)", key, ttl)
}

// Delete removes an entry. Returns true if the key was present.
func (c *KVCache) Delete(key string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	_, ok := c.data[key]
	if ok {
		delete(c.data, key)
		c.logger.Debugf("KVCache.Delete %q", key)
	}
	return ok
}

// Len reports the current number of entries (including expired-but-not-evicted).
func (c *KVCache) Len() int {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.data)
}

// evictOldest removes a single arbitrary entry. Called while holding c.mu.
func (c *KVCache) evictOldest() {
	for k := range c.data {
		delete(c.data, k)
		return
	}
}

// ── snapshot.go ──
// Snapshot represents a serializable view of a KVCache for backup/restore.
type Snapshot struct {
	Entries map[string]string `json:"entries"`
}

// SnapshotKVCache walks every entry of c and returns a Snapshot. Expired
// entries are skipped. The returned snapshot is independent of the source
// cache and may be persisted or shipped over the wire.
func SnapshotKVCache(c *KVCache) *Snapshot {
	c.mu.RLock()
	defer c.mu.RUnlock()
	snap := &Snapshot{Entries: make(map[string]string, len(c.data))}
	for k, e := range c.data {
		if !e.expiresAt.IsZero() {
			continue // skip TTL'd entries
		}
		snap.Entries[k] = e.value
	}
	return snap
}

// RestoreKVCache loads a Snapshot's entries into c, overwriting collisions.
func RestoreKVCache(c *KVCache, snap *Snapshot) {
	for k, v := range snap.Entries {
		c.Set(k, v)
	}
}

// WriteSnapshot serializes a Snapshot as JSON to the writer.
func WriteSnapshot(w io.Writer, snap *Snapshot) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	if err := enc.Encode(snap); err != nil {
		return fmt.Errorf("write snapshot: %w", err)
	}
	return nil
}

// ReadSnapshot decodes a Snapshot previously written by WriteSnapshot.
func ReadSnapshot(r io.Reader) (*Snapshot, error) {
	var snap Snapshot
	if err := json.NewDecoder(r).Decode(&snap); err != nil {
		return nil, fmt.Errorf("read snapshot: %w", err)
	}
	return &snap, nil
}

// ── admin.go ──
// AdminTool exposes ad-hoc commands for inspecting a KVCache from the
// admin REPL or CLI.
type AdminTool struct {
	cache  *KVCache
	logger *Logger
}

// NewAdminTool builds an AdminTool bound to the given KVCache.
func NewAdminTool(cache *KVCache, logger *Logger) *AdminTool {
	return &AdminTool{cache: cache, logger: logger}
}

// Execute runs a single admin command. Recognized commands:
//
//	get <key>
//	set <key> <value>
//	delete <key>
//	size
func (a *AdminTool) Execute(line string) (string, error) {
	fields := strings.Fields(line)
	if len(fields) == 0 {
		return "", fmt.Errorf("empty command")
	}
	switch fields[0] {
	case "get":
		if len(fields) != 2 {
			return "", fmt.Errorf("usage: get <key>")
		}
		v, ok := a.cache.Get(fields[1])
		if !ok {
			return "<missing>", nil
		}
		return v, nil
	case "set":
		if len(fields) != 3 {
			return "", fmt.Errorf("usage: set <key> <value>")
		}
		a.cache.Set(fields[1], fields[2])
		return "ok", nil
	case "delete":
		if len(fields) != 2 {
			return "", fmt.Errorf("usage: delete <key>")
		}
		if !a.cache.Delete(fields[1]) {
			return "<missing>", nil
		}
		return "ok", nil
	case "size":
		return fmt.Sprintf("%d", a.cache.Len()), nil
	default:
		return "", fmt.Errorf("unknown command: %s", fields[0])
	}
}

// ── api.go ──
// API is a thin HTTP layer over a KVCache instance.
type API struct {
	cache     *KVCache
	validator *Validator
	logger    *Logger
}

// NewAPI constructs an API handler bound to the given KVCache.
func NewAPI(cache *KVCache, validator *Validator, logger *Logger) *API {
	return &API{cache: cache, validator: validator, logger: logger}
}

// Routes returns an http.Handler with all KV endpoints registered.
func (a *API) Routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/get", a.handleGet)
	mux.HandleFunc("/set", a.handleSet)
	mux.HandleFunc("/delete", a.handleDelete)
	mux.HandleFunc("/stats", a.handleStats)
	return mux
}

func (a *API) handleGet(w http.ResponseWriter, r *http.Request) {
	key := r.URL.Query().Get("key")
	if err := a.validator.Key(key); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	value, ok := a.cache.Get(key)
	if !ok {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}
	a.logger.Infof("api: GET %q -> hit", key)
	json.NewEncoder(w).Encode(map[string]string{"key": key, "value": value})
}

func (a *API) handleSet(w http.ResponseWriter, r *http.Request) {
	key := r.URL.Query().Get("key")
	value := r.URL.Query().Get("value")
	ttlStr := r.URL.Query().Get("ttl_seconds")
	if err := a.validator.KeyValue(key, value); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if ttlStr != "" {
		secs, err := strconv.Atoi(ttlStr)
		if err != nil || secs < 0 {
			http.Error(w, "invalid ttl_seconds", http.StatusBadRequest)
			return
		}
		a.cache.SetTTL(key, value, time.Duration(secs)*time.Second)
	} else {
		a.cache.Set(key, value)
	}
	a.logger.Infof("api: SET %q", key)
	w.WriteHeader(http.StatusNoContent)
}

func (a *API) handleDelete(w http.ResponseWriter, r *http.Request) {
	key := r.URL.Query().Get("key")
	if err := a.validator.Key(key); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if !a.cache.Delete(key) {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (a *API) handleStats(w http.ResponseWriter, r *http.Request) {
	stats := map[string]int{
		"size": a.cache.Len(),
	}
	json.NewEncoder(w).Encode(stats)
}

// ── main.go ──
// Server bundles all KVCache-backed components into a runnable unit.
type Server struct {
	cache *KVCache
	api   *API
}

// NewServer wires together a KVCache, validator, logger, metrics, and HTTP API.
// maxSize=0 means an unbounded KVCache.
func NewServer(maxSize int) *Server {
	logger := NewLogger("kvproj")
	metrics := NewMetrics()
	cache := NewKVCache(maxSize, logger, metrics)
	validator := NewValidator(256, 65536)
	api := NewAPI(cache, validator, logger)
	return &Server{cache: cache, api: api}
}

// ListenAndServe starts the HTTP API on the given address.
func (s *Server) ListenAndServe(addr string) error {
	srv := &http.Server{
		Addr:         addr,
		Handler:      s.api.Routes(),
		ReadTimeout:  5 * time.Second,
		WriteTimeout: 5 * time.Second,
	}
	return srv.ListenAndServe()
}

// Cache returns the underlying KVCache for tests and admin tooling.
func (s *Server) Cache() *KVCache {
	return s.cache
}
