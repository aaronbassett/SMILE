# Getting Started with Docker Containers

This tutorial walks you through your first Docker container.

## Prerequisites

- A 64-bit operating system
- Administrative access on your machine

## Step 1: Verify Docker Installation

Check that Docker is installed:

```bash
docker --version
```

You should see output like `Docker version 24.0.0, build abcdef`.

## Step 2: Pull Your First Image

Pull the official hello-world image:

```bash
docker pull hello-world
```

This downloads a small test image from Docker Hub.

## Step 3: Run the Container

Run the hello-world container:

```bash
docker run hello-world
```

You should see a message starting with "Hello from Docker!".

## Step 4: List Running Containers

See what containers are running:

```bash
docker ps
```

You'll notice hello-world isn't listed because it exits immediately.

## Step 5: List All Containers

To see all containers including stopped ones:

```bash
docker ps -a
```

You should see your hello-world container with status "Exited".

## Step 6: Run an Interactive Container

Start an Ubuntu container interactively:

```bash
docker run -it ubuntu bash
```

You're now inside the container! Run some commands:

```bash
cat /etc/os-release
echo "Hello from inside Docker!"
exit
```

## Step 7: Clean Up

Remove the stopped containers:

```bash
docker container prune -f
```

## Conclusion

You've learned the basics of Docker! Next steps:
- Build your own images with Dockerfile
- Use Docker Compose for multi-container apps
- Explore Docker networking
