# OpenShift Deployment
This document describes the steps to launch FishNet on an OpenShift 4 cluster.

## Steps
1. Create a new project. `project.yaml` can be used.
2. Create a secret with your FishNet API key. Number of CPU cores to be used can also be set here.
3. Create a new deployment using the provided `deployment.yaml` file.
4. Set the replicas/pods to your liking.

### 1. Create a new project 
We assume, that you are already logged in to your OCP cluster. For simplicity the steps shown in this document refer to the commandline, but can also be done via the OPC GUI.

There are two simple ways to create a new project:

- `oc new-project fishnet` (this command also changes to the project automatically)

or use the provided `project.yaml` file
- `oc apply -f project.yaml` (you need to change to the new project manually: `oc project fishnet`)

### 2. Create a Secret with your FishNet API key
Create a secret named `fishnet`:

`oc create secret generic fishnet -n fishnet --from-literal KEY="<Your_API_Key>"`

By default _FishNet_ sets the number of cores to be used to `n-1`. So if you dont want the worker to use this value, you could add a `CORES` value to the secret like this:

`oc create secret generic fishnet -n fishnet --from-literal KEY="<Your_API_Key>" --from-literal CORES="1"`
