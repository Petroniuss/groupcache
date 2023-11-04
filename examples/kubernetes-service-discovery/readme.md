# kubernetes-service-discovery example

## Running service on groupcache

Make sure all dependencies are installed and running (see below).

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

```bash
kubectl apply -f k8s/groupcache-powered-backend-role.yaml
kubectl apply -f k8s/groupcache-powered-backend-role-binding.yaml
kubectl apply -f k8s/groupcache-powered-backend-deployment.yaml
```

POD_IP is reachable from other pods.
Command to run to curl pods from inside the cluster:
```bash
kubectl run mycurlpod --image=curlimages/curl -i --tty -- sh
```

Find groupcache pod ips:
```bash
kubectl get pod -o wide
```

Inside the curl pod run:
```bash
curl ${GRPUCACHE_POD_ID}:3000
```

To cleanup curl pod run
```bash
kubectl exec -i --tty mycurlpod -- sh
```
or 
```bash
kubectl delete pod mycurlpod
```

To get all pods:
```bash
kubectl get pods --selector=app=groupcache-powered-backend -o wide
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
minikube image load groupcache-powered-backend:latest
```