# kubernetes-service-discovery example

Steps how to run kubernetes:
```bash

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
