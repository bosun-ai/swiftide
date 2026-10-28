# Goal

A coding agent has made a tool call. It is your job to refine the output.

Reformat the following tool output such that it is effective for an AI agent to work with. Only include the reformatted output in your response.
{% if prompt_to_summarize -%}
The output is too large and needs to be summarized. Please summarize the output
and only include the most important information that is relevant to the task at hand.

When mentioning files, include the full path to the file.
{% else %}
Reformat but do not summarize, all information should be preserved and detailed.
{% endif %}

## Tool call information

Tool name: {{tool_name}}
Tool description: {{tool_description}}
Tool was called with arguments: {{tool_args}}

{% if tool_name == 'run_tests' or tool_name == 'run_coverage' -%}

If the tests pass, additionally mention that coverage must be checked such that
it actually improved, did not stay the same, and the file executed properly.
{% endif -%}
{% if diff -%}

## The agent has made the following changes

````diff
{{diff}}
````

{% endif %}

## Tool output

````shell
{{tool_output}}
````

## Format

- Only include the reformatted output in your response and nothing else.
- Include clear instructions on how to fix each issue using the tools that are
  available only.
- If you do not have a clear solution, state that you do not have a clear solution.
- If there is any mangling in the tool response, reformat it to be readable.
{% if diff -%}
- If you suspect that any changes made by the agent affect the tool output, mention
  that. Make sure you include full paths to the files.
{% endif %}

## Available tools

{{formatted_tools}}

## Requirements

- Only propose improvements that can be fixed by the tools and functions that are
  available in the conversation. For instance, running a command to fix linting can also be fixed by writing to that file without errors.
- If the tool output has repeating patterns, only include the pattern once and state
  that it happens multiple times.
- You cannot call tools yourself. You can only propose changes that can be made by
  the agent.
