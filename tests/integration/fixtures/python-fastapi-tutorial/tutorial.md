# Building a FastAPI Hello World Server

This tutorial shows you how to build a simple REST API server.

## Prerequisites

- Basic Python knowledge
- A terminal/command prompt

## Step 1: Create Project Directory

Create a new directory for your project:

```bash
mkdir fastapi-hello && cd fastapi-hello
```

## Step 2: Create Virtual Environment

Create a Python virtual environment:

```bash
python -m venv venv
```

Activate the virtual environment:

```bash
source venv/bin/activate
```

## Step 3: Install FastAPI

Install FastAPI with pip:

```bash
pip install fastapi uvicorn
```

## Step 4: Create the Server

Create a file called `main.py` with the following content:

```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def read_root():
    return {"Hello": "World"}

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str = None):
    return {"item_id": item_id, "q": q}
```

## Step 5: Run the Server

Start the server:

```bash
uvicorn main:app --reload
```

You should see output indicating the server is running.

## Step 6: Test the API

Open a new terminal and run:

```bash
curl http://localhost:8000/
```

You should see `{"Hello":"World"}` in the response.

## Step 7: Add Documentation

FastAPI generates automatic API documentation. Visit these URLs in your browser:

- Swagger UI: http://localhost:8000/docs
- ReDoc: http://localhost:8000/redoc

## Conclusion

You've created a basic FastAPI server! Try extending it with:
- Database integration
- Authentication
- Request validation
