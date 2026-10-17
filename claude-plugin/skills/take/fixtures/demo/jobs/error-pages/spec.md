# Error pages

Show a page for each HTTP error. The server returns **plain HTML**, never a *stack trace*. The `ErrorPageFilter` picks the page. See the [HTTP status codes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status) for the full list.

## Pages

| Status | Page | Message |
|---|---|---|
| 404 | `404.html` | Page not found |
| 500 | `500.html` | Something went wrong |
| 503 | `503.html` | Back soon |

## Filter

```java
public final class ErrorPageFilter implements Filter {

  @Override
  public void doFilter(Request request, Response response) {
    int status = response.status();
    if (status >= 400) {
      response.body(ErrorPages.forStatus(status));
    }
  }
}
```

### Tasks

- [x] Add the 404 page
- [ ] Add the 500 page

### Layout

- Header
  - Logo
  - Search box
- Body
  - Status code
  - Message
- Footer

### Steps

1. Match the status code.
2. Load the page.
3. Write the response.

> Never show internal details on an error page.
> Log them instead.

<style>header { display: none }</style>

<script>alert('script ran')</script>
<img src="x" onerror="alert('onerror ran')">
