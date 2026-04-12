package kvproj

import (
	"encoding/json"
	"net/http"
	"strconv"
	"time"
)

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
