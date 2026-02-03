"""
Real-world examples of using the LLM CLI wrapper.

Demonstrates:
1. Simple synchronous wrapper around Claude CLI
2. Async multi-turn conversations
3. JSON-structured outputs with validation
4. Error handling and recovery
5. FastAPI integration patterns
6. Code analysis and review workflows
"""

import asyncio
import json
import logging
from datetime import datetime
from typing import Optional, List

from pydantic import BaseModel, Field

from llm_cli_wrapper import (
    LLMCliWrapper,
    LLMError,
    ErrorCategory,
    RetryConfig,
    TimeoutConfig,
    ConversationManager,
    JSONExtractor,
)

# Configure logging for examples
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


# ============================================================================
# Example 1: Code Analysis Wrapper
# ============================================================================

class CodeIssue(BaseModel):
    """A detected code issue."""
    line_number: int
    severity: str = Field(..., description="critical|warning|info")
    message: str
    suggestion: Optional[str] = None


class CodeAnalysisResult(BaseModel):
    """Result of code analysis."""
    file_name: str
    language: str
    summary: str
    issues: List[CodeIssue] = Field(default_factory=list)
    maintainability_score: float = Field(..., ge=0, le=100)
    recommendations: List[str] = Field(default_factory=list)


class CodeAnalyzer:
    """Analyze code using Claude CLI."""

    def __init__(
        self,
        cli_path: str = "claude",
        timeout_seconds: float = 60.0
    ):
        self.wrapper = LLMCliWrapper(
            cli_path=cli_path,
            timeout_config=TimeoutConfig(total_execution=timeout_seconds)
        )

    async def analyze_code(
        self,
        code: str,
        language: str = "python",
        include_suggestions: bool = True
    ) -> CodeAnalysisResult:
        """
        Analyze code for issues and quality.

        Args:
            code: Source code to analyze
            language: Programming language
            include_suggestions: Whether to include improvement suggestions

        Returns:
            CodeAnalysisResult with issues and recommendations
        """

        # Build analysis prompt with JSON schema
        schema = CodeAnalysisResult.model_json_schema()
        schema_json = json.dumps(schema, indent=2)

        prompt = f"""Analyze this {language} code for issues and provide structured feedback.

Code to analyze:
```{language}
{code}
```

Respond ONLY with valid JSON matching this schema:
{schema_json}

Focus on:
1. Runtime errors and bugs
2. Performance issues
3. Security vulnerabilities
4. Code style and maintainability
5. Error handling gaps

For each issue, include:
- Line number (estimate if unclear)
- Severity (critical/warning/info)
- Clear message describing the issue
- Specific suggestion to fix it

Provide a maintainability score 0-100 (higher is better)."""

        try:
            logger.info(f"Analyzing {language} code ({len(code)} chars)")

            # Build input for CLI
            input_data = json.dumps({
                "model": "claude-3-5-sonnet",
                "messages": [
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0  # Deterministic for structured output
            })

            # Call Claude API
            result = await self.wrapper.call_api_async(
                cmd_args=["api", "call"],
                input_data=input_data,
                output_model=CodeAnalysisResult
            )

            logger.info(f"Analysis complete: {len(result.get('issues', []))} issues found")
            return CodeAnalysisResult(**result)

        except LLMError as e:
            logger.error(f"Analysis failed: {e}")
            raise

    def analyze_code_sync(
        self,
        code: str,
        language: str = "python"
    ) -> CodeAnalysisResult:
        """Synchronous version of analyze_code."""

        schema = CodeAnalysisResult.model_json_schema()
        schema_json = json.dumps(schema, indent=2)

        prompt = f"""Analyze this {language} code:

```{language}
{code}
```

Respond ONLY with valid JSON matching this schema:
{schema_json}"""

        try:
            input_data = json.dumps({
                "model": "claude-3-5-sonnet",
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0
            })

            result = self.wrapper.call_api_sync(
                cmd_args=["api", "call"],
                input_data=input_data,
                output_model=CodeAnalysisResult
            )

            return CodeAnalysisResult(**result)

        except LLMError as e:
            logger.error(f"Sync analysis failed: {e}")
            raise


# ============================================================================
# Example 2: Multi-Turn Code Review
# ============================================================================

class CodeReviewSession:
    """Manage multi-turn code review conversation."""

    def __init__(self, code: str, language: str = "python"):
        self.code = code
        self.language = language
        self.manager = ConversationManager(
            system_prompt=f"""You are an expert code reviewer specializing in {language}.
Your role is to:
1. Identify bugs and potential issues
2. Suggest performance improvements
3. Review code style and maintainability
4. Explain trade-offs in design decisions

Be constructive and specific in your feedback.""",
            max_turns=5
        )

    async def ask_about_code(self, question: str) -> str:
        """
        Ask a question about the code in context.

        Args:
            question: Specific question about the code

        Returns:
            Review response
        """

        # Add code context to first question
        if self.manager.context.turn_count == 0:
            full_question = f"""I have this {self.language} code:

```{self.language}
{self.code}
```

{question}"""
        else:
            full_question = question

        async def call_llm(prompt: str) -> str:
            """Call Claude with the prompt."""
            input_data = json.dumps({
                "model": "claude-3-5-sonnet",
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.3  # Some creativity for explanations
            })

            from llm_cli_wrapper import LLMCliWrapper
            wrapper = LLMCliWrapper("claude")
            result = await wrapper.call_api_async(
                cmd_args=["api", "call"],
                input_data=input_data
            )
            return result

        try:
            response = await self.manager.send_message(
                full_question,
                call_llm
            )
            logger.info(f"Review turn {self.manager.context.turn_count}: Got response")
            return response

        except LLMError as e:
            logger.error(f"Code review failed: {e}")
            raise

    def export_review(self, filepath: str) -> None:
        """Export the entire review session."""
        self.manager.export_history(filepath)
        logger.info(f"Review exported to {filepath}")


# ============================================================================
# Example 3: Batch Processing with Error Handling
# ============================================================================

class BatchCodeAnalyzer:
    """Analyze multiple code files with error handling."""

    def __init__(self, max_concurrent: int = 3):
        self.analyzer = CodeAnalyzer()
        self.max_concurrent = max_concurrent
        self.results = []
        self.errors = []

    async def analyze_batch(self, files: dict[str, str]) -> dict:
        """
        Analyze multiple code files.

        Args:
            files: Dict mapping filename -> code content

        Returns:
            {
                'succeeded': [CodeAnalysisResult, ...],
                'failed': [{'file': str, 'error': str}, ...]
            }
        """

        # Use semaphore to limit concurrent calls
        semaphore = asyncio.Semaphore(self.max_concurrent)

        async def analyze_with_semaphore(filename: str, code: str):
            async with semaphore:
                try:
                    logger.info(f"Starting analysis: {filename}")
                    result = await self.analyzer.analyze_code(
                        code,
                        language=self._detect_language(filename)
                    )
                    self.results.append({
                        "file": filename,
                        "result": result
                    })
                    logger.info(f"Completed: {filename}")

                except LLMError as e:
                    logger.error(f"Failed to analyze {filename}: {e}")
                    self.errors.append({
                        "file": filename,
                        "error": str(e),
                        "category": e.category.value
                    })

        # Run all analyses concurrently
        tasks = [
            analyze_with_semaphore(filename, code)
            for filename, code in files.items()
        ]

        await asyncio.gather(*tasks)

        return {
            "succeeded": [
                r["result"].model_dump()
                for r in self.results
            ],
            "failed": self.errors
        }

    @staticmethod
    def _detect_language(filename: str) -> str:
        """Detect programming language from filename."""
        ext_map = {
            ".py": "python",
            ".js": "javascript",
            ".ts": "typescript",
            ".java": "java",
            ".rs": "rust",
            ".go": "go",
            ".cpp": "cpp",
            ".c": "c"
        }

        for ext, lang in ext_map.items():
            if filename.endswith(ext):
                return lang

        return "python"  # Default


# ============================================================================
# Example 4: Streaming Documentation Generation
# ============================================================================

class DocumentationGenerator:
    """Generate documentation with streaming output."""

    def __init__(self, cli_path: str = "claude"):
        self.wrapper = LLMCliWrapper(cli_path)

    async def generate_docs_streaming(
        self,
        code: str,
        language: str = "python"
    ) -> str:
        """
        Generate documentation with streaming output.

        Args:
            code: Source code to document
            language: Programming language

        Yields:
            Documentation chunks as they're generated
        """

        prompt = f"""Generate comprehensive documentation for this {language} code:

```{language}
{code}
```

Include:
1. Overview of functionality
2. Function/class descriptions
3. Parameter and return value documentation
4. Usage examples
5. Performance considerations"""

        input_data = json.dumps({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.3
        })

        # For now, return collected response
        # In a real implementation, this would stream chunks
        result = await self.wrapper.call_api_async(
            cmd_args=["api", "call"],
            input_data=input_data
        )

        return result


# ============================================================================
# Example 5: Retry and Circuit Breaking Demo
# ============================================================================

async def demo_resilient_analysis():
    """Demonstrate resilient error handling."""

    # Configure with aggressive retry settings for demo
    config = RetryConfig(
        max_attempts=3,
        initial_delay_ms=50,
        exponential_base=2.0
    )

    timeout_config = TimeoutConfig(total_execution=120)

    analyzer = CodeAnalyzer(timeout_seconds=120)
    analyzer.wrapper.retry_handler = analyzer.wrapper.retry_handler.__class__(config)

    # Example code with potential issues
    example_code = """
def process_data(items):
    for item in items:
        if item > 0:
            result = 100 / item
            print(result)
    return result  # Undefined outside loop

class DataProcessor:
    def __init__(self):
        self.data = None

    def process(self):
        return self.data + 10  # Potential TypeError
"""

    try:
        logger.info("Starting resilient analysis with retries")
        result = await analyzer.analyze_code(example_code)

        print("\n=== Code Analysis Results ===")
        print(f"Maintainability Score: {result.maintainability_score}/100")
        print(f"Issues Found: {len(result.issues)}")

        for issue in result.issues:
            print(f"\n  Line {issue.line_number} [{issue.severity.upper()}]")
            print(f"  Message: {issue.message}")
            if issue.suggestion:
                print(f"  Suggestion: {issue.suggestion}")

        if result.recommendations:
            print(f"\n=== Recommendations ===")
            for rec in result.recommendations:
                print(f"  - {rec}")

    except LLMError as e:
        logger.error(f"Analysis failed: {e}")
        logger.error(f"Category: {e.category.value}")


# ============================================================================
# Example 6: Conversation-Based Code Review
# ============================================================================

async def demo_interactive_review():
    """Demonstrate interactive multi-turn code review."""

    code_to_review = """
def calculate_average(numbers):
    total = 0
    for num in numbers:
        total = total + num
    average = total / len(numbers)
    return average

class DataCache:
    def __init__(self):
        self.cache = {}

    def get(self, key):
        return self.cache.get(key)

    def set(self, key, value):
        self.cache[key] = value
"""

    try:
        session = CodeReviewSession(code_to_review, "python")

        # First question
        logger.info("Starting interactive code review")
        response1 = await session.ask_about_code(
            "What are the main issues in this code?"
        )
        print("\nQ1: What are the main issues?\n")
        print(response1)

        # Second question
        response2 = await session.ask_about_code(
            "How would you improve the average calculation?"
        )
        print("\nQ2: How to improve average calculation?\n")
        print(response2)

        # Export session
        session.export_review("/tmp/code_review.json")
        logger.info("Review session exported")

    except LLMError as e:
        logger.error(f"Review session failed: {e}")


# ============================================================================
# Example 7: Batch Processing Multiple Files
# ============================================================================

async def demo_batch_analysis():
    """Demonstrate batch processing."""

    files = {
        "utils.py": """
def merge_dicts(d1, d2):
    result = {}
    for k in d1:
        result[k] = d1[k]
    for k in d2:
        result[k] = d2[k]
    return result
""",
        "cache.py": """
class SimpleCache:
    def __init__(self, max_size=100):
        self.max_size = max_size
        self.items = []

    def get(self, key):
        for item in self.items:
            if item[0] == key:
                return item[1]
        return None
""",
        "api.py": """
import requests

def fetch_data(url):
    response = requests.get(url)
    return response.json()
"""
    }

    try:
        logger.info(f"Starting batch analysis of {len(files)} files")

        batch = BatchCodeAnalyzer(max_concurrent=2)
        results = await batch.analyze_batch(files)

        print("\n=== Batch Analysis Results ===")
        print(f"Succeeded: {len(results['succeeded'])}")
        print(f"Failed: {len(results['failed'])}")

        for success in results['succeeded']:
            print(f"\n  {success['file']}")
            print(f"    Score: {success.get('maintainability_score', 'N/A')}/100")

        if results['failed']:
            print("\n  Failed analyses:")
            for failure in results['failed']:
                print(f"    {failure['file']}: {failure['error']}")

    except Exception as e:
        logger.error(f"Batch processing failed: {e}")


# ============================================================================
# Main: Run Examples
# ============================================================================

async def main():
    """Run all examples."""

    print("=" * 60)
    print("LLM CLI Wrapper - Real-World Examples")
    print("=" * 60)

    # Note: These examples require Claude CLI to be installed and configured
    # Uncomment to run actual examples

    # Example 1: Code Analysis
    # print("\n[Example 1] Code Analysis")
    # await demo_resilient_analysis()

    # Example 2: Interactive Review
    # print("\n[Example 2] Interactive Code Review")
    # await demo_interactive_review()

    # Example 3: Batch Processing
    # print("\n[Example 3] Batch Processing")
    # await demo_batch_analysis()

    print("\nNote: Run examples by uncommenting in main() function")
    print("Requires: claude CLI tool installed and configured")


if __name__ == "__main__":
    asyncio.run(main())
