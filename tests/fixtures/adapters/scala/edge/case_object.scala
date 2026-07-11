// Scala ADT idiom: 'case object' leaves are public, but the adapter has no
// 'case object' rule and 'case' is not a stripped modifier -> leaves MISSED.
package example.edge

sealed trait Status

case object Active extends Status

case object Inactive extends Status
