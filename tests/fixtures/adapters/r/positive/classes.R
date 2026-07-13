Animal <- R6Class("Animal",
  public = list(
    name = NULL,
    speak = function() {
      "..."
    }
  )
)

setClass("Token",
  slots = c(value = "character", ttl = "numeric")
)

setGeneric("expired", function(token) standardGeneric("expired"))
