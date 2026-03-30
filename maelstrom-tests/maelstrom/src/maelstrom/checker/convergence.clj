(ns maelstrom.checker.convergence
  "Checks that cache invalidation converges within a bounded time.

  Algorithm:
  1. Track the 'current value' for each key (set by load, cleared by invalidate)
  2. After each invalidate, any read returning the old value is 'stale'
  3. Convergence latency = time of last stale read - time of invalidation
  4. Failure = stale read persists beyond delta-t after invalidation"
  (:require [jepsen [checker :as checker]
                    [util :as util]]))

(def default-delta-t-nanos
  "Default convergence threshold in nanoseconds.
  Override via DELTA_T_MS environment variable (milliseconds)."
  (if-let [env-ms (System/getenv "DELTA_T_MS")]
    (* (Long/parseLong env-ms) 1000 1000)
    (* 100 1000 1000)))

(defn completed-ops
  "Filter to only completed (ok) operations."
  [history]
  (filter #(= :ok (:type %)) history))

(defn build-key-timeline
  "Build a timeline of events per key from the history.
  Returns {key [{:time t :op :load/:read/:invalidate :value v} ...]}"
  [history]
  (reduce
    (fn [acc op]
      (let [key    (get-in op [:value :key])
            result (get-in op [:value :result])
            entry  {:time  (:time op)
                    :op    (:f op)
                    :value result}]
        (update acc key (fnil conj []) entry)))
    {}
    (completed-ops history)))

(defn analyze-key
  "Analyze convergence for a single key's timeline.
  Returns a map with :stale-reads and :convergence-latencies."
  [events]
  (loop [remaining        events
         current-value    nil
         pre-inv-value    nil
         inv-time         nil
         stale-reads      []
         conv-latencies   []]
    (if (empty? remaining)
      {:stale-reads           stale-reads
       :convergence-latencies conv-latencies}
      (let [event (first remaining)
            rest  (rest remaining)]
        (case (:op event)
          :load
          (recur rest (:value event) pre-inv-value inv-time stale-reads conv-latencies)

          :invalidate
          (recur rest nil current-value (:time event) stale-reads conv-latencies)

          :read
          (if (and inv-time
                   pre-inv-value
                   (:value event)
                   (= (:value event) pre-inv-value))
            ;; Stale read detected
            (let [latency-ns (- (:time event) inv-time)]
              (recur rest current-value pre-inv-value inv-time
                     (conj stale-reads {:time       (:time event)
                                        :latency-ns latency-ns
                                        :value      (:value event)})
                     (conj conv-latencies latency-ns)))
            ;; Normal read
            (recur rest current-value pre-inv-value inv-time stale-reads conv-latencies))

          ;; Unknown op, skip
          (recur rest current-value pre-inv-value inv-time stale-reads conv-latencies))))))

(defn percentile
  "Compute the p-th percentile of a sorted sequence."
  [sorted-vals p]
  (if (empty? sorted-vals)
    0
    (let [idx (min (dec (count sorted-vals))
                   (int (Math/ceil (* (/ p 100.0) (count sorted-vals)))))]
      (nth sorted-vals idx))))

(defn ns->ms
  "Convert nanoseconds to milliseconds (double)."
  [ns]
  (/ (double ns) 1000000.0))

(defn checker
  "Create a convergence checker.
  Options:
    :delta-t-nanos  Maximum allowed convergence time (default 5s)"
  ([] (checker {}))
  ([opts]
   (let [delta-t (get opts :delta-t-nanos default-delta-t-nanos)]
     (reify checker/Checker
       (check [this test history check-opts]
         (let [timelines     (build-key-timeline history)
               analyses      (map (fn [[k events]] [k (analyze-key events)]) timelines)
               all-latencies (->> analyses
                                  (mapcat (fn [[_ a]] (:convergence-latencies a)))
                                  sort
                                  vec)
               all-stale     (->> analyses
                                  (mapcat (fn [[_ a]] (:stale-reads a)))
                                  vec)
               violations    (filter #(> (:latency-ns %) delta-t) all-stale)
               valid?        (empty? violations)]
           {:valid?       valid?
            :convergence  (when (seq all-latencies)
                            {:p50-ms (ns->ms (percentile all-latencies 50))
                             :p95-ms (ns->ms (percentile all-latencies 95))
                             :p99-ms (ns->ms (percentile all-latencies 99))
                             :max-ms (ns->ms (last all-latencies))
                             :count  (count all-latencies)})
            :stale-reads  {:total      (count all-stale)
                           :violations (count violations)}
            :delta-t-ms   (ns->ms delta-t)}))))))
