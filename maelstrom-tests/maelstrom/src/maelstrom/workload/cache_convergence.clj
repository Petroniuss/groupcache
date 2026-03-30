(ns maelstrom.workload.cache-convergence
  "A workload for testing cache invalidation convergence.

  Operations:
    - load:       Force a key to be loaded (generates a new versioned value)
    - read:       Read a key from any cache tier
    - invalidate: Remove a key from all caches

  The checker verifies that after invalidation + delta-t, all nodes
  converge (no stale reads persist)."
  (:require [maelstrom [client :as c]
                       [net :as net]]
            [maelstrom.checker.convergence :as convergence]
            [jepsen [checker :as checker]
                    [client :as client]
                    [generator :as gen]]
            [schema.core :as s]
            [slingshot.slingshot :refer [try+ throw+]]))

(def key-count
  "Number of distinct keys to test."
  10)

;; ── RPC definitions ─────────────────────────────────────────────────

(c/defrpc load-key!
  "Force a key to be loaded on the owning node."
  {:type (s/eq "load")
   :key  s/Str}
  {:type  (s/eq "load_ok")
   :value s/Any})

(c/defrpc read-key
  "Read a key from any cache tier."
  {:type (s/eq "read")
   :key  s/Str}
  {:type  (s/eq "read_ok")
   :value s/Any})

(c/defrpc invalidate-key!
  "Invalidate a key across all nodes."
  {:type (s/eq "invalidate")
   :key  s/Str}
  {:type (s/eq "invalidate_ok")})

;; ── Client ──────────────────────────────────────────────────────────

(defn client
  "Construct a cache-convergence client for the given network."
  ([net]
   (client net nil nil))
  ([net conn node]
   (reify client/Client
     (open! [this test node]
       (client net (c/open! net) node))

     (setup! [this test])

     (invoke! [_ test op]
       (let [k (get-in op [:value :key])]
         (c/with-errors op #{:read}
           (case (:f op)
             :load
             (let [res (load-key! conn node {:key k})]
               (assoc op :type :ok
                      :value (assoc (:value op) :result (:value res))))

             :read
             (let [res (read-key conn node {:key k})]
               (assoc op :type :ok
                      :value (assoc (:value op) :result (:value res))))

             :invalidate
             (do (invalidate-key! conn node {:key k})
                 (assoc op :type :ok))))))

     (teardown! [_ test])

     (close! [_ test]
       (c/close! conn)))))

;; ── Generator ───────────────────────────────────────────────────────

(defn load-op   [_ _] {:type :invoke, :f :load,       :value {:key (str "k" (rand-int key-count))}})
(defn read-op   [_ _] {:type :invoke, :f :read,       :value {:key (str "k" (rand-int key-count))}})
(defn inv-op    [_ _] {:type :invoke, :f :invalidate, :value {:key (str "k" (rand-int key-count))}})

;; ── Workload ────────────────────────────────────────────────────────

(defn workload
  "Constructs a workload for cache convergence testing.

      {:net     A Maelstrom network}"
  [opts]
  {:client    (client (:net opts))
   :generator (gen/mix [load-op read-op read-op read-op inv-op])
   :checker   (convergence/checker)})
