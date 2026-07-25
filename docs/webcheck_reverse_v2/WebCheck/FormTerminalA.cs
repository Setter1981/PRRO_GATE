using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using CardPay;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormTerminalA : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ButtonClose")]
	private Button _ButtonClose;

	[CompilerGenerated]
	[AccessedThroughProperty("Timer1")]
	private Timer _Timer1;

	[CompilerGenerated]
	[AccessedThroughProperty("Timer2")]
	private Timer _Timer2;

	private TypCardserv RRR;

	private const string NameFormT = "Оплата карткою                                                  ";

	private PosApi Aval;

	private int TTT;

	private bool FlagID;

	private int LimitTimeUp;

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PictureBox1")]
	internal virtual PictureBox PictureBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBoxOrder")]
	internal virtual TextBox TextBoxOrder
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TextBoxSum")]
	internal virtual TextBox TextBoxSum
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ButtonClose
	{
		[CompilerGenerated]
		get
		{
			return _ButtonClose;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ButtonClose_Click;
			Button buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				buttonClose.Click -= value2;
			}
			_ButtonClose = value;
			buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				buttonClose.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxStatus")]
	internal virtual TextBox TextBoxStatus
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Timer Timer1
	{
		[CompilerGenerated]
		get
		{
			return _Timer1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Timer1_Tick;
			Timer timer = _Timer1;
			if (timer != null)
			{
				timer.Tick -= value2;
			}
			_Timer1 = value;
			timer = _Timer1;
			if (timer != null)
			{
				timer.Tick += value2;
			}
		}
	}

	internal virtual Timer Timer2
	{
		[CompilerGenerated]
		get
		{
			return _Timer2;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Timer2_Tick;
			Timer timer = _Timer2;
			if (timer != null)
			{
				timer.Tick -= value2;
			}
			_Timer2 = value;
			timer = _Timer2;
			if (timer != null)
			{
				timer.Tick += value2;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		this.components = new System.ComponentModel.Container();
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormTerminalA));
		this.Label2 = new System.Windows.Forms.Label();
		this.PictureBox1 = new System.Windows.Forms.PictureBox();
		this.TextBoxOrder = new System.Windows.Forms.TextBox();
		this.TextBoxSum = new System.Windows.Forms.TextBox();
		this.ButtonClose = new System.Windows.Forms.Button();
		this.TextBoxStatus = new System.Windows.Forms.TextBox();
		this.Timer1 = new System.Windows.Forms.Timer(this.components);
		this.Timer2 = new System.Windows.Forms.Timer(this.components);
		((System.ComponentModel.ISupportInitialize)this.PictureBox1).BeginInit();
		base.SuspendLayout();
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(14, 177);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(165, 25);
		this.Label2.TabIndex = 17;
		this.Label2.Text = "Статус операції:";
		this.PictureBox1.Image = (System.Drawing.Image)resources.GetObject("PictureBox1.Image");
		this.PictureBox1.Location = new System.Drawing.Point(14, 12);
		this.PictureBox1.Name = "PictureBox1";
		this.PictureBox1.Size = new System.Drawing.Size(305, 101);
		this.PictureBox1.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
		this.PictureBox1.TabIndex = 16;
		this.PictureBox1.TabStop = false;
		this.TextBoxOrder.Font = new System.Drawing.Font("Microsoft Sans Serif", 18f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBoxOrder.Location = new System.Drawing.Point(14, 119);
		this.TextBoxOrder.Multiline = true;
		this.TextBoxOrder.Name = "TextBoxOrder";
		this.TextBoxOrder.ReadOnly = true;
		this.TextBoxOrder.Size = new System.Drawing.Size(356, 44);
		this.TextBoxOrder.TabIndex = 15;
		this.TextBoxOrder.TabStop = false;
		this.TextBoxSum.Font = new System.Drawing.Font("Consolas", 19.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBoxSum.Location = new System.Drawing.Point(411, 118);
		this.TextBoxSum.Multiline = true;
		this.TextBoxSum.Name = "TextBoxSum";
		this.TextBoxSum.ReadOnly = true;
		this.TextBoxSum.Size = new System.Drawing.Size(356, 43);
		this.TextBoxSum.TabIndex = 14;
		this.TextBoxSum.TabStop = false;
		this.TextBoxSum.TextAlign = System.Windows.Forms.HorizontalAlignment.Right;
		this.ButtonClose.Anchor = System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left;
		this.ButtonClose.Font = new System.Drawing.Font("Microsoft Sans Serif", 18f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ButtonClose.Location = new System.Drawing.Point(14, 423);
		this.ButtonClose.Name = "ButtonClose";
		this.ButtonClose.Size = new System.Drawing.Size(753, 62);
		this.ButtonClose.TabIndex = 13;
		this.ButtonClose.TabStop = false;
		this.ButtonClose.Text = "ЗАКРИТИ";
		this.ButtonClose.UseVisualStyleBackColor = true;
		this.TextBoxStatus.Font = new System.Drawing.Font("Microsoft Sans Serif", 18f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBoxStatus.Location = new System.Drawing.Point(14, 205);
		this.TextBoxStatus.Multiline = true;
		this.TextBoxStatus.Name = "TextBoxStatus";
		this.TextBoxStatus.ReadOnly = true;
		this.TextBoxStatus.Size = new System.Drawing.Size(753, 197);
		this.TextBoxStatus.TabIndex = 12;
		this.TextBoxStatus.TabStop = false;
		this.TextBoxStatus.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(780, 497);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.PictureBox1);
		base.Controls.Add(this.TextBoxOrder);
		base.Controls.Add(this.TextBoxSum);
		base.Controls.Add(this.ButtonClose);
		base.Controls.Add(this.TextBoxStatus);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormTerminalA";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Оплата карткою";
		base.TopMost = true;
		((System.ComponentModel.ISupportInitialize)this.PictureBox1).EndInit();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormTerminalA(TypCardserv e)
	{
		base.Load += FormTerminalA_Load;
		base.Closing += FormTerminalA_Closing;
		Aval = new PosApi();
		FlagID = false;
		LimitTimeUp = 35000;
		InitializeComponent();
		RRR = e;
	}

	private void FormTerminalA_Load(object sender, EventArgs e)
	{
		LimitTimeUp = All.f.GetInteger("Global", "TerminalTimeout", 0);
		if (LimitTimeUp < 9)
		{
			LimitTimeUp = 35;
			All.f.WriteInteger("Global", "TerminalTimeout", LimitTimeUp);
		}
		base.AcceptButton = ButtonClose;
		TextBoxOrder.Text = All.MethodEnToUa(RRR.method).ToUpper();
		TextBoxSum.Text = RRR.amount;
		if (Operators.CompareString(RRR.dest[0].ToString().ToLower(), "c", TextCompare: false) != 0)
		{
			RRR.dest = RRR.dest + ":" + Conversions.ToString(RRR.port);
		}
		if (Aval.posOpen(RRR.dest, ""))
		{
			TS("Термінал готовий до використання");
			StartComand(TestTransID());
			int num = checked(All.f.GetInteger("Global", "TerminalTimeoutUnlockBut", -1) * 1000);
			if (num < 0)
			{
				num = 0;
				All.f.WriteInteger("Global", "TerminalTimeoutUnlockBut", num);
			}
			if (num > 0)
			{
				Timer2.Interval = num;
				Timer2.Start();
			}
		}
		else
		{
			TS("Помилка з'єднання з терміналом");
			Response(ErrorBool: true, "Помилка відкриття порту " + RRR.dest);
			All.LgT.SaveTextToLogCardserv("PORT OPEN ERROR", RRR.dest);
		}
	}

	private string TestTransID()
	{
		return All.iTA.GetString("General", "POS_TRANS_ID");
	}

	private void StartComand(string eTransID = "")
	{
		FlagID = false;
		switch (RRR.method)
		{
		case "purchase":
			if (Operators.CompareString(eTransID, "", TextCompare: false) == 0)
			{
				Aval.posSet(PosApi.POS_AMOUNT, All.Bablo(RRR.amount, sDot: false));
				Aval.posSet(PosApi.POS_MERCHANT_ID, RRR.merchantId);
				Aval.posSet(PosApi.POS_CURRENCY, "980");
				Aval.posSend(PosApi.Action.PAYMENT);
			}
			else
			{
				FlagID = true;
				Aval.posSet(PosApi.POS_MERCHANT_ID, RRR.merchantId);
				Aval.posSet(PosApi.POS_TRANS_ID, eTransID);
				Aval.posSend(PosApi.Action.STATUS);
				All.LgT.SaveTextToLogCardserv("Відновлення транзакції", "Розпочато відновлення транзакції № " + eTransID);
			}
			break;
		case "refund":
			Aval.posSet(PosApi.POS_AMOUNT, All.Bablo(RRR.amount, sDot: false));
			Aval.posSet(PosApi.POS_MERCHANT_ID, RRR.merchantId);
			Aval.posSet(PosApi.POS_TRANS_CODE, RRR.rrn);
			Aval.posSet(PosApi.POS_CURRENCY, "980");
			Aval.posSend(PosApi.Action.RETURN);
			break;
		case "verify":
			Aval.posSend(PosApi.Action.CLOSE_DAY);
			break;
		case "audit":
			Aval.posSet(PosApi.POS_REPORT, "1");
			Aval.posSend(PosApi.Action.REPORT);
			break;
		case "withdrawal":
			Aval.posSet(PosApi.POS_AMOUNT, All.Bablo(RRR.amount, sDot: false));
			Aval.posSet(PosApi.POS_MERCHANT_ID, RRR.merchantId);
			Aval.posSet(PosApi.POS_TRANS_RECEIPT, RRR.invoicenumber);
			Aval.posSet(PosApi.POS_CURRENCY, "980");
			Aval.posSend(PosApi.Action.REVERSAL);
			break;
		case "cashback":
			Aval.posSet(PosApi.POS_AMOUNT, All.Bablo(RRR.amount, sDot: false));
			Aval.posSet(PosApi.POS_MERCHANT_ID, RRR.merchantId);
			Aval.posSet(PosApi.POS_CURRENCY, "980");
			Aval.posSend(PosApi.Action.CASH);
			break;
		case "identify":
			Aval.posSend(PosApi.Action.POS_INFO);
			break;
		default:
			TS("Помилковий метод!");
			Timer1.Stop();
			return;
		}
		Timer1.Interval = 1000;
		Timer1.Start();
		TTT = LimitTimeUp;
		ButtonClose.Enabled = false;
	}

	private void TS(string el, string el2 = "", string el3 = "")
	{
		if (el2.Length > 0)
		{
			el = el + Environment.NewLine + el2;
		}
		if (el3.Length > 0)
		{
			el = el + Environment.NewLine + el3;
		}
		TextBoxStatus.Text = el;
	}

	private void Response(bool ErrorBool = false, string ErrorDes = "")
	{
		if (Operators.CompareString(ErrorDes, "", TextCompare: false) == 0)
		{
			ErrorDes = Aval.posGet(PosApi.POS_TRANS_ID) + " " + Aval.posGet("status_code");
		}
		All.B.CurrentStatus = "error=" + ErrorBool + "_errordescription=" + ErrorDes;
		All.B.CurrentStatus = All.B.CurrentStatus + "_amount=" + Aval.posGet(PosApi.POS_AMOUNT) + "_paymentsystem=" + Aval.posGet(PosApi.POS_CARD_PAYMENT_SYS) + "_bankacquirer=" + Aval.posGet(PosApi.POS_BANK_ACQUIRER) + "_approvalcode=" + Aval.posGet(PosApi.POS_TRANS_APPROVAL) + "_invoicenumber=" + Aval.posGet(PosApi.POS_TRANS_RECEIPT) + "_merchant=" + Aval.posGet(PosApi.POS_MERCHANT_ID) + "_pan=" + Aval.posGet(PosApi.POS_CARD_PAN) + "_rrn=" + Aval.posGet(PosApi.POS_TRANS_CODE) + "_terminalid=" + Aval.posGet(PosApi.POS_TERMINAL_ID) + "_merchantId=" + RRR.merchantId;
		All.B.CurrentStatus = All.B.CurrentStatus + "_posserial=" + Aval.posGet(PosApi.POS_SERIAL_NUMBER) + "_possoftver=" + Aval.posGet(PosApi.POS_SOFT_VER) + "_cashbackamount=" + Aval.posGet(PosApi.POS_CASHBACK_AMOUNT);
		All.B.CurrentStatus = All.B.CurrentStatus + "_pts=" + Aval.posGet(PosApi.POS_DATE_TIME);
		All.B.CurrentStatus = All.B.CurrentStatus + "_posapitransid=" + Aval.posGet(PosApi.POS_TRANS_ID);
		All.CardservTrue = !ErrorBool;
		All.LgT.SaveTextToLogCardserv("Response PosApi", All.B.CurrentStatus);
	}

	private void Timer1_Tick(object sender, EventArgs e)
	{
		PosApi.Response response = Aval.posReceive(9);
		switch (response)
		{
		case PosApi.Response.CONFIRM:
			if (FlagID & (Operators.CompareString(RRR.method, "purchase", TextCompare: false) == 0))
			{
				if (Operators.CompareString(Aval.posGet(PosApi.POS_AMOUNT), All.Bablo(RRR.amount, sDot: false), TextCompare: false) == 0)
				{
					Response();
					TS("Операцію підтверджено.", "Закрийте вікно...");
					All.LgT.SaveTextToLogCardserv("Відновлення транзакції", "Успіх");
					ButtonClose.Enabled = true;
					Timer1.Stop();
					Close();
				}
				else
				{
					All.B.CurrentStatus = "error=true_errordescription=Операція перервана терміналом. Повторіть операцію.";
					TS("Операція перервана терміналом.", "Повторіть операцію...");
					All.CardservTrue = false;
					All.LgT.SaveTextToLogCardserv("Відновлення транзакції", "Помилка верифікації сум", Aval.posGet(PosApi.POS_AMOUNT) + "<>" + All.Bablo(RRR.amount, sDot: false));
					ButtonClose.Enabled = true;
					Timer1.Stop();
				}
			}
			else if (Operators.CompareString(RRR.method, "identify", TextCompare: false) == 0)
			{
				ButtonClose.Enabled = true;
				Timer1.Stop();
				Response();
				TS("Операцію підтверджено.", "pos serial: " + Aval.posGet(PosApi.POS_SERIAL_NUMBER), "pos ver.: " + Aval.posGet(PosApi.POS_SOFT_VER));
			}
			else
			{
				Response();
				TS("Операцію підтверджено.", "Закрийте вікно...");
				ButtonClose.Enabled = true;
				Timer1.Stop();
				Close();
			}
			Aval.posClose();
			break;
		case PosApi.Response.INPUT:
			ButtonClose.Enabled = true;
			Timer1.Stop();
			TS("INPUT");
			break;
		case PosApi.Response.KEEPALIVE:
			ButtonClose.Enabled = true;
			Timer1.Stop();
			TS("KEEPALIVE");
			break;
		case PosApi.Response.IDENTIFIER:
			if (Operators.CompareString(RRR.method, "purchase", TextCompare: false) == 0)
			{
				All.iTA.WriteString("General", "POS_TRANS_ID", Aval.posGet(PosApi.POS_TRANS_ID));
			}
			else
			{
				All.iTA.WriteString("General", "POS_TRANS_ID", "");
			}
			break;
		case PosApi.Response.MESSAGE:
			TS(Aval.posGet(PosApi.POS_MSG_TITLE), Aval.posGet(PosApi.POS_MSG_BODY));
			break;
		case PosApi.Response.BREAK:
			Response(ErrorBool: true, "Операція перервана терміналом. Повторіть операцію.");
			TS("BREAK");
			Timer1.Stop();
			Aval.posClose();
			ButtonClose.Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.WRONG_MAC:
			Response(ErrorBool: true, "Операція відкинута через неправильний мак поля");
			TS("WRONG_MAC");
			Timer1.Stop();
			Aval.posClose();
			ButtonClose.Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.DECLINE:
			Response(ErrorBool: true, "Відмова банку");
			TS("DECLINE");
			TS(Aval.posGet(PosApi.POS_TRANS_STATUS));
			Timer1.Stop();
			Aval.posClose();
			ButtonClose.Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.ERROR:
			Response(ErrorBool: true, "Помилка робота з драйвером терміналу POSAPI");
			TS(Conversions.ToString((int)response) + " - ERROR");
			Timer1.Stop();
			Aval.posClose();
			ButtonClose.Enabled = true;
			Timer1.Stop();
			break;
		default:
			TS("Непередбачувана відповідь терміналу - " + Conversions.ToString((int)response));
			ButtonClose.Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.TIMEOUT:
			break;
		}
		checked
		{
			if (TTT <= 0)
			{
				ButtonClose.Enabled = true;
				Timer1.Stop();
				Aval.posClose();
			}
			else
			{
				TTT--;
				Text = "Оплата карткою                                                  " + TTT;
			}
		}
	}

	private void ButtonClose_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void FormTerminalA_Closing(object sender, CancelEventArgs e)
	{
		if ((Timer1.Enabled & ButtonClose.Enabled) && Interaction.MsgBox("Вікно буде закрито. Термінал продовжить працювати без керування програмою. Перевірте що на терміналі не виконується трансакція.", MsgBoxStyle.Exclamation | MsgBoxStyle.OkCancel, "Увага") == MsgBoxResult.Cancel)
		{
			e.Cancel = true;
			return;
		}
		e.Cancel = !ButtonClose.Enabled;
		if (ButtonClose.Enabled)
		{
			Timer1.Stop();
			Aval.posClose();
		}
	}

	private void Timer2_Tick(object sender, EventArgs e)
	{
		ButtonClose.Enabled = true;
		Timer2.Stop();
	}
}
