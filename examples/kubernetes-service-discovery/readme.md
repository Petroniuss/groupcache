# kubernetes-service-discovery example

This example shows how to use groupcache in a distributed setting, see [./src/main.rs](./src/main.rs).


## Running service on groupcache

Make sure all dependencies are installed and running (see below).


```bash
kubectl apply -f k8s/groupcache-powered-backend/role.yaml
kubectl apply -f k8s/groupcache-powered-backend/role-binding.yaml
kubectl apply -f k8s/groupcache-powered-backend/deployment.yaml
```

## Dependencies

### Minikube

Download minikube using your preferred method https://minikube.sigs.k8s.io/docs/start/.

Start kubernetes locally:
```bash
minikube start
```

### Backend docker image

To build example as a docker image run:
```bash
docker build . -t groupcache-powered-backend:latest
```

Load image in minikube;
```bash
minikube image load groupcache-powered-backend:latest
```

### Observability
Optional, but allows you to see what is going on in the cluster and see metrics exported by groupcache:
- `groupcache.*`

And metrics exported from axum via prometheus_exporter crate:
- `axum_*`

There is a prepared dashboard for HTTP requests in grafana called `groupcache-powered-backend`.

Installing prometheus:
```bash
helm install -f k8s/prometheus-community/values.yaml prometheus prometheus-community/prometheus --version 25.3.1
```

Installing grafana:
```bash
helm install -f k8s/grafana/values.yaml grafana grafana/grafana --version 6.61.1
```

To access grafana run:
`minikube service grafana`
