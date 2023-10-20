# kubernetes-service-discovery example

## Running service on groupcache

Make sure all dependencies are installed and running (see below).

```bash
kubectl apply -f k8s/groupcache-powered-backend-deployment.yaml
```

## Dependencies

### Minikube

Download minikube using your preferred method https://minikube.sigs.k8s.io/docs/start/.

Start kubernetes locally:
```bash
minikube start
```

To proxy requests to api server through localhost:8080 run:
```bash
kubectl proxy --port=8080
```

accessing kubernetes api:
https://kubernetes.io/docs/tasks/administer-cluster/access-cluster-api/
https://stackoverflow.com/questions/40720979/how-to-access-kubernetes-api-when-using-minkube

kubectl config location:
'/Users/pwojtyczek/.kube/config'

### Building backend

To build example as a docker image run:
```bash
docker build . -t groupcache-powered-backend
```

Load image in minikube;
```bash
minikube image load groupcache-powered-backend
```
