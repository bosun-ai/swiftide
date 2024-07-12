{% include "src/transformers/prompts/file_to_context_task.prompt.md" %}

This task has already been performed on the previous chunks of code. The summary so far is:

            ```
            {summary_so_far}
            ```

            This is the next chunk of code, please give your response following the rules above without further
            commentary or explanation:

            ```
            {current_chunk}
            ```
