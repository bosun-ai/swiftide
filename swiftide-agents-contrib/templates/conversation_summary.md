
# Goal

Summarize and review the conversation up to this point. An agent has tried to achieve a goal, and you need to help them get there. Always instruct the agent to continue and finish the task.

# Original goal of this conversation for reference

{{ initial_user_message }}

## Requirements

* Only include the summary in your response and nothing else.
* If any, mention every file changed
* When mentioning files include the full path
* Be very precise and critical
* If the agent was reading files in order to understand a solution, provide a detailed
    summary of any specific relevant code which will be useful for the agents next steps
    such that the agent can work directly from the summarized code without having to reread
    the files. Do not summarize code or files which are not directly relevant to the agents
    next steps. When summarizing code include the exact names of objects and functions, as
    well as detailed explanations of what they are used for and how they work. Also provide
    the snippets of the actual code which is relevant to the agents next steps such that
    the agent will not have to reread the files. Do not include entire files, only relevant
    snippets of code.
* If a previous solution did not work, include that in your response. If a reason was
    given, include that as well.
* Include any previous summaries in your response
* Include every step so far taken and clearly state where the agent is at,
    especially in relation to the initial goal. Include any observations or information
    made, and include your own where relevant to achieving the goal.
* Be extra detailed on the last steps taken
* Provide clear instructions on how to proceed. If applicable, include the tools that
    should be used.
* Identify the bigger goal the user wanted to achieve and clearly restate it
* If the goal is not yet achieved, reflect on why and provide a clear path forward
* In suggested next steps, talk in the future tense. For example: \"You should run the tests\"
* Do not provide the actual tool calls the agent should still make, provide the steps and
    necessary context instead. Assume the agent knows how to use the tools.

{% if diff | length > 0  -%}

## Current changes made

````
{{ diff }}
````

{% endif %}

## Available tools

{{available_tools}}

{% if plan is defined and plan | length > 0 %}

## Current plan

{% for item in plan -%}
* [{{ item.status }}] {{ item.step }}
{% endfor %}

{% endif %}

## Format

* Start your response with the following header '# Summary'
* Phrase each bullet point as if talking about 'you'

# Example format

```
# Summary

## Your goal
<Your goal>

## Since then you did
* <summary of each step>
* You tried to run the tests but they failed. Here is why <...>
* You read a file called `full/path/to/file.txt` and here is what you learned <...>

## Relevant files
<Summary of relevant code which was read including the exact names of objects and functions
  also include snippets of the actual code which is relevant to the agents next steps such that the agent will not have
  to reread the files. Do not include entire files, only relevant snippets of code.>  

## Reflection
<Reflection on the steps you took and why you took them. What have you observed and what have you learned so far>

## Suggested next steps
1. <Suggested step>
```
