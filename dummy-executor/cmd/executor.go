package cmd

import (
	"fmt"
	"log"
	"os"
	"os/signal"
	"strconv"
	"syscall"

	"github.com/quickfixgo/enum"
	"github.com/quickfixgo/field"
	fix44er "github.com/quickfixgo/fix44/executionreport"
	"github.com/quickfixgo/fix44/newordersingle"
	"github.com/quickfixgo/quickfix"
	"github.com/quickfixgo/quickfix/log/screen"
	"github.com/shopspring/decimal"
)

// Realistic FX mid-market rates for common currency pairs.
var fxRates = map[string]decimal.Decimal{
	"EUR/USD": decimal.NewFromFloat(1.0850),
	"GBP/USD": decimal.NewFromFloat(1.2650),
	"USD/JPY": decimal.NewFromFloat(149.50),
	"USD/CHF": decimal.NewFromFloat(0.8820),
	"AUD/USD": decimal.NewFromFloat(0.6540),
	"USD/CAD": decimal.NewFromFloat(1.3580),
	"NZD/USD": decimal.NewFromFloat(0.6120),
	"EUR/GBP": decimal.NewFromFloat(0.8580),
	"EUR/JPY": decimal.NewFromFloat(162.20),
	"GBP/JPY": decimal.NewFromFloat(189.10),
}

// Executor implements quickfix.Application and handles incoming FIX messages.
type Executor struct {
	orderID int
	execID  int
	*quickfix.MessageRouter
}

func newExecutor() *Executor {
	e := &Executor{MessageRouter: quickfix.NewMessageRouter()}
	e.AddRoute(newordersingle.Route(e.onNewOrderSingle))
	return e
}

func (e *Executor) genOrderID() string {
	e.orderID++
	return strconv.Itoa(e.orderID)
}

func (e *Executor) genExecID() string {
	e.execID++
	return strconv.Itoa(e.execID)
}

// OnCreate is called when a FIX session is created.
func (e *Executor) OnCreate(sessionID quickfix.SessionID) {
	log.Printf("Session created: %s", sessionID)
}

// OnLogon is called when a FIX session logs on.
func (e *Executor) OnLogon(sessionID quickfix.SessionID) {
	log.Printf("Session logon: %s", sessionID)
}

// OnLogout is called when a FIX session logs out.
func (e *Executor) OnLogout(sessionID quickfix.SessionID) {
	log.Printf("Session logout: %s", sessionID)
}

// ToAdmin is called for outgoing admin messages.
func (e *Executor) ToAdmin(msg *quickfix.Message, sessionID quickfix.SessionID) {}

// ToApp is called for outgoing application messages.
func (e *Executor) ToApp(msg *quickfix.Message, sessionID quickfix.SessionID) error { return nil }

// FromAdmin is called for incoming admin messages.
func (e *Executor) FromAdmin(msg *quickfix.Message, sessionID quickfix.SessionID) quickfix.MessageRejectError {
	return nil
}

// FromApp is called for incoming application messages and routes them.
func (e *Executor) FromApp(msg *quickfix.Message, sessionID quickfix.SessionID) quickfix.MessageRejectError {
	return e.Route(msg, sessionID)
}

func (e *Executor) onNewOrderSingle(msg newordersingle.NewOrderSingle, sessionID quickfix.SessionID) quickfix.MessageRejectError {
	clOrdID, err := msg.GetClOrdID()
	if err != nil {
		return err
	}

	symbol, err := msg.GetSymbol()
	if err != nil {
		return err
	}

	side, err := msg.GetSide()
	if err != nil {
		return err
	}

	orderQty, err := msg.GetOrderQty()
	if err != nil {
		return err
	}

	log.Printf("Received NewOrderSingle: ClOrdID=%s Symbol=%s Side=%s Qty=%s",
		clOrdID, symbol, string(side), orderQty.String())

	// Read the optional custom tag (6001 = ClientStrategyId).
	var clientStrategyID quickfix.FIXInt
	hasClientStrategyID := false
	if err := msg.Body.GetField(quickfix.Tag(6001), &clientStrategyID); err == nil {
		hasClientStrategyID = true
		log.Printf("  ClientStrategyId=%d", int(clientStrategyID))
	}

	// Look up FX rate; default to 1.0000 for unknown pairs.
	price, ok := fxRates[symbol]
	if !ok {
		price = decimal.NewFromFloat(1.0000)
		log.Printf("Unknown symbol %s, using default rate 1.0000", symbol)
	}

	orderID := e.genOrderID()
	zero := decimal.NewFromInt(0)

	// --- ACK (New) ---
	ack := fix44er.New(
		field.NewOrderID(orderID),
		field.NewExecID(e.genExecID()),
		field.NewExecType(enum.ExecType_NEW),
		field.NewOrdStatus(enum.OrdStatus_NEW),
		field.NewSide(side),
		field.NewLeavesQty(orderQty, 2),
		field.NewCumQty(zero, 2),
		field.NewAvgPx(zero, 2),
	)
	ack.Set(field.NewClOrdID(clOrdID))
	ack.Set(field.NewSymbol(symbol))
	ack.Set(field.NewOrderQty(orderQty, 2))
	if hasClientStrategyID {
		ack.Body.SetField(quickfix.Tag(6001), clientStrategyID)
	}

	if sendErr := quickfix.SendToTarget(ack.ToMessage(), sessionID); sendErr != nil {
		log.Printf("Error sending ACK: %v", sendErr)
	} else {
		log.Printf("Sent ACK for OrderID=%s", orderID)
	}

	// --- FILL ---
	fill := fix44er.New(
		field.NewOrderID(orderID),
		field.NewExecID(e.genExecID()),
		field.NewExecType(enum.ExecType_TRADE),
		field.NewOrdStatus(enum.OrdStatus_FILLED),
		field.NewSide(side),
		field.NewLeavesQty(zero, 2),
		field.NewCumQty(orderQty, 2),
		field.NewAvgPx(price, 4),
	)
	fill.Set(field.NewClOrdID(clOrdID))
	fill.Set(field.NewSymbol(symbol))
	fill.Set(field.NewOrderQty(orderQty, 2))
	fill.Set(field.NewLastQty(orderQty, 2))
	fill.Set(field.NewLastPx(price, 4))
	if hasClientStrategyID {
		fill.Body.SetField(quickfix.Tag(6001), clientStrategyID)
	}

	if sendErr := quickfix.SendToTarget(fill.ToMessage(), sessionID); sendErr != nil {
		log.Printf("Error sending FILL: %v", sendErr)
	} else {
		log.Printf("Sent FILL for OrderID=%s at %s", orderID, price.String())
	}

	return nil
}

// Run starts the FIX acceptor with the given config file path and blocks until interrupted.
func Run(cfgFileName string) error {
	cfg, err := os.Open(cfgFileName)
	if err != nil {
		return fmt.Errorf("open config: %w", err)
	}
	defer func(cfg *os.File) {
		err := cfg.Close()
		if err != nil {
			log.Printf("Error closing config file: %v", err)
		}
	}(cfg)

	appSettings, err := quickfix.ParseSettings(cfg)
	if err != nil {
		return fmt.Errorf("parse settings: %w", err)
	}

	app := newExecutor()
	storeFactory := quickfix.NewMemoryStoreFactory()
	logFactory := screen.NewLogFactory()

	acceptor, err := quickfix.NewAcceptor(app, storeFactory, appSettings, logFactory)
	if err != nil {
		return fmt.Errorf("create acceptor: %w", err)
	}

	if err := acceptor.Start(); err != nil {
		return fmt.Errorf("start acceptor: %w", err)
	}
	log.Printf("FIX acceptor started on port 9880, waiting for connections...")

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	<-sig

	log.Println("Shutting down...")
	acceptor.Stop()
	return nil
}
