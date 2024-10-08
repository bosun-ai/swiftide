# Filtering Code Outline

Your task is to filter the given file outline to the code chunk provided. The goal is to provide a context that is still contains the lines needed for understanding the code in the chunk whilst leaving out any irrelevant information.

## Constraints

- Only use lines from the provided context, do not add any additional information
- Ensure that the selection you make is the most appropriate for the code chunk
- Make sure you include any definitions or imports that are used in the code chunk
- You do not need to repeat the code chunk in your response, it will be appended directly after your response.
- Do not use lines that are present in the code chunk

## Code

```
{{ node.chunk }}
```

## Outline

```
{{ node.metadata["Outline"] }}
```
