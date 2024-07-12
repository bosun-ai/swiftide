{% include "src/transformers/prompts/file_to_context_task.prompt.md" %}

For example, given the following code:
```
example.py
import hashlib

def foo():
    # This is a comment
    return 1

def bar():
    if True:
```

Your response should be:
```
Python example.py
import hashlib
def foo():
def bar():
PARTIAL
```

Another example, now in Java:
```
example.java
import java.util.*;

public class Main {
    public static void main(String[] args) {
        // This is a comment
        System.out.println("Hello, World!");
    }
}
```

Your response should be:
```
Java example.java
import java.util.*;
public class Main {
    public static void main(String[] args)
}
```

This is the first chunk of code, please give your response following the rules above without further
commentary or explanation:

```
{file_name}
{current_chunk}
```
