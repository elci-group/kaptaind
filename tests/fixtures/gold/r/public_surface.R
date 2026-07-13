# Package: geometry

Point <- R6Class("Point",
  public = list(
    x = 0,
    y = 0
  )
)

distance <- function(a, b) {
  sqrt((a$x - b$x)^2 + (a$y - b$y)^2)
}

area = function(shape, ...) {
  0
}

setGeneric("describe", function(obj) standardGeneric("describe"))

.helper <- function(x) {
  x * 2
}
