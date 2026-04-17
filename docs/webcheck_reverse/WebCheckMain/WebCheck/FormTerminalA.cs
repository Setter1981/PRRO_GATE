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
			EventHandler eventHandler = ButtonClose_Click;
			Button buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				((Control)buttonClose).Click -= eventHandler;
			}
			_ButtonClose = value;
			buttonClose = _ButtonClose;
			if (buttonClose != null)
			{
				((Control)buttonClose).Click += eventHandler;
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
			EventHandler eventHandler = Timer1_Tick;
			Timer timer = _Timer1;
			if (timer != null)
			{
				timer.Tick -= eventHandler;
			}
			_Timer1 = value;
			timer = _Timer1;
			if (timer != null)
			{
				timer.Tick += eventHandler;
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
			EventHandler eventHandler = Timer2_Tick;
			Timer timer = _Timer2;
			if (timer != null)
			{
				timer.Tick -= eventHandler;
			}
			_Timer2 = value;
			timer = _Timer2;
			if (timer != null)
			{
				timer.Tick += eventHandler;
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
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_0064: Unknown result type (might be due to invalid IL or missing references)
		//IL_006e: Expected O, but got Unknown
		//IL_0075: Unknown result type (might be due to invalid IL or missing references)
		//IL_007f: Expected O, but got Unknown
		//IL_00b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_00bd: Expected O, but got Unknown
		//IL_0129: Unknown result type (might be due to invalid IL or missing references)
		//IL_0133: Expected O, but got Unknown
		//IL_01aa: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b4: Expected O, but got Unknown
		//IL_0237: Unknown result type (might be due to invalid IL or missing references)
		//IL_0241: Expected O, but got Unknown
		//IL_02df: Unknown result type (might be due to invalid IL or missing references)
		//IL_02e9: Expected O, but got Unknown
		//IL_0373: Unknown result type (might be due to invalid IL or missing references)
		//IL_037d: Expected O, but got Unknown
		//IL_04a5: Unknown result type (might be due to invalid IL or missing references)
		//IL_04af: Expected O, but got Unknown
		components = new Container();
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTerminalA));
		Label2 = new Label();
		PictureBox1 = new PictureBox();
		TextBoxOrder = new TextBox();
		TextBoxSum = new TextBox();
		ButtonClose = new Button();
		TextBoxStatus = new TextBox();
		Timer1 = new Timer(components);
		Timer2 = new Timer(components);
		((ISupportInitialize)PictureBox1).BeginInit();
		((Control)this).SuspendLayout();
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(14, 177);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(165, 25);
		((Control)Label2).TabIndex = 17;
		Label2.Text = "Статус операції:";
		PictureBox1.Image = (Image)componentResourceManager.GetObject("PictureBox1.Image");
		((Control)PictureBox1).Location = new Point(14, 12);
		((Control)PictureBox1).Name = "PictureBox1";
		((Control)PictureBox1).Size = new Size(305, 101);
		PictureBox1.SizeMode = (PictureBoxSizeMode)4;
		PictureBox1.TabIndex = 16;
		PictureBox1.TabStop = false;
		((Control)TextBoxOrder).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxOrder).Location = new Point(14, 119);
		TextBoxOrder.Multiline = true;
		((Control)TextBoxOrder).Name = "TextBoxOrder";
		((TextBoxBase)TextBoxOrder).ReadOnly = true;
		((Control)TextBoxOrder).Size = new Size(356, 44);
		((Control)TextBoxOrder).TabIndex = 15;
		((Control)TextBoxOrder).TabStop = false;
		((Control)TextBoxSum).Font = new Font("Consolas", 19.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxSum).Location = new Point(411, 118);
		TextBoxSum.Multiline = true;
		((Control)TextBoxSum).Name = "TextBoxSum";
		((TextBoxBase)TextBoxSum).ReadOnly = true;
		((Control)TextBoxSum).Size = new Size(356, 43);
		((Control)TextBoxSum).TabIndex = 14;
		((Control)TextBoxSum).TabStop = false;
		TextBoxSum.TextAlign = (HorizontalAlignment)1;
		((Control)ButtonClose).Anchor = (AnchorStyles)6;
		((Control)ButtonClose).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ButtonClose).Location = new Point(14, 423);
		((Control)ButtonClose).Name = "ButtonClose";
		((Control)ButtonClose).Size = new Size(753, 62);
		((Control)ButtonClose).TabIndex = 13;
		((Control)ButtonClose).TabStop = false;
		((ButtonBase)ButtonClose).Text = "ЗАКРИТИ";
		((ButtonBase)ButtonClose).UseVisualStyleBackColor = true;
		((Control)TextBoxStatus).Font = new Font("Microsoft Sans Serif", 18f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBoxStatus).Location = new Point(14, 205);
		TextBoxStatus.Multiline = true;
		((Control)TextBoxStatus).Name = "TextBoxStatus";
		((TextBoxBase)TextBoxStatus).ReadOnly = true;
		((Control)TextBoxStatus).Size = new Size(753, 197);
		((Control)TextBoxStatus).TabIndex = 12;
		((Control)TextBoxStatus).TabStop = false;
		TextBoxStatus.TextAlign = (HorizontalAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(780, 497);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)PictureBox1);
		((Control)this).Controls.Add((Control)(object)TextBoxOrder);
		((Control)this).Controls.Add((Control)(object)TextBoxSum);
		((Control)this).Controls.Add((Control)(object)ButtonClose);
		((Control)this).Controls.Add((Control)(object)TextBoxStatus);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTerminalA";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Оплата карткою";
		((Form)this).TopMost = true;
		((ISupportInitialize)PictureBox1).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormTerminalA(TypCardserv e)
	{
		((Form)this).Load += FormTerminalA_Load;
		((Form)this).Closing += FormTerminalA_Closing;
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
		((Form)this).AcceptButton = (IButtonControl)(object)ButtonClose;
		TextBoxOrder.Text = All.MethodEnToUa(RRR.method).ToUpper();
		TextBoxSum.Text = RRR.amount;
		if (Operators.CompareString(RRR.dest[0].ToString().ToLower(), "c", false) != 0)
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
			if (Operators.CompareString(eTransID, "", false) == 0)
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
		((Control)ButtonClose).Enabled = false;
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
		if (Operators.CompareString(ErrorDes, "", false) == 0)
		{
			ErrorDes = Aval.posGet(PosApi.POS_TRANS_ID) + " " + Aval.posGet(PosApi.POS_STATUS);
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
			if (FlagID & (Operators.CompareString(RRR.method, "purchase", false) == 0))
			{
				if (Operators.CompareString(Aval.posGet(PosApi.POS_AMOUNT), All.Bablo(RRR.amount, sDot: false), false) == 0)
				{
					Response();
					TS("Операцію підтверджено.", "Закрийте вікно...");
					All.LgT.SaveTextToLogCardserv("Відновлення транзакції", "Успіх");
					((Control)ButtonClose).Enabled = true;
					Timer1.Stop();
					((Form)this).Close();
				}
				else
				{
					All.B.CurrentStatus = "error=true_errordescription=Операція перервана терміналом. Повторіть операцію.";
					TS("Операція перервана терміналом.", "Повторіть операцію...");
					All.CardservTrue = false;
					All.LgT.SaveTextToLogCardserv("Відновлення транзакції", "Помилка верифікації сум", Aval.posGet(PosApi.POS_AMOUNT) + "<>" + All.Bablo(RRR.amount, sDot: false));
					((Control)ButtonClose).Enabled = true;
					Timer1.Stop();
				}
			}
			else if (Operators.CompareString(RRR.method, "identify", false) == 0)
			{
				((Control)ButtonClose).Enabled = true;
				Timer1.Stop();
				Response();
				TS("Операцію підтверджено.", "pos serial: " + Aval.posGet(PosApi.POS_SERIAL_NUMBER), "pos ver.: " + Aval.posGet(PosApi.POS_SOFT_VER));
			}
			else
			{
				Response();
				TS("Операцію підтверджено.", "Закрийте вікно...");
				((Control)ButtonClose).Enabled = true;
				Timer1.Stop();
				((Form)this).Close();
			}
			Aval.posClose();
			break;
		case PosApi.Response.INPUT:
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			TS("INPUT");
			break;
		case PosApi.Response.KEEPALIVE:
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			TS("KEEPALIVE");
			break;
		case PosApi.Response.IDENTIFIER:
			if (Operators.CompareString(RRR.method, "purchase", false) == 0)
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
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.WRONG_MAC:
			Response(ErrorBool: true, "Операція відкинута через неправильний мак поля");
			TS("WRONG_MAC");
			Timer1.Stop();
			Aval.posClose();
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.DECLINE:
			Response(ErrorBool: true, "Відмова банку");
			TS("DECLINE");
			TS(Aval.posGet(PosApi.POS_TRANS_STATUS));
			Timer1.Stop();
			Aval.posClose();
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.ERROR:
			Response(ErrorBool: true, "Помилка робота з драйвером терміналу POSAPI");
			TS(Conversions.ToString((int)response) + " - ERROR");
			Timer1.Stop();
			Aval.posClose();
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			break;
		default:
			TS("Непередбачувана відповідь терміналу - " + Conversions.ToString((int)response));
			((Control)ButtonClose).Enabled = true;
			Timer1.Stop();
			break;
		case PosApi.Response.TIMEOUT:
			break;
		}
		checked
		{
			if (TTT <= 0)
			{
				((Control)ButtonClose).Enabled = true;
				Timer1.Stop();
				Aval.posClose();
			}
			else
			{
				TTT--;
				((Form)this).Text = "Оплата карткою                                                  " + TTT;
			}
		}
	}

	private void ButtonClose_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void FormTerminalA_Closing(object sender, CancelEventArgs e)
	{
		//IL_0025: Unknown result type (might be due to invalid IL or missing references)
		//IL_002b: Invalid comparison between Unknown and I4
		if ((Timer1.Enabled & ((Control)ButtonClose).Enabled) && (int)Interaction.MsgBox((object)"Вікно буде закрито. Термінал продовжить працювати без керування програмою. Перевірте що на терміналі не виконується трансакція.", (MsgBoxStyle)49, (object)"Увага") == 2)
		{
			e.Cancel = true;
			return;
		}
		e.Cancel = !((Control)ButtonClose).Enabled;
		if (((Control)ButtonClose).Enabled)
		{
			Timer1.Stop();
			Aval.posClose();
		}
	}

	private void Timer2_Tick(object sender, EventArgs e)
	{
		((Control)ButtonClose).Enabled = true;
		Timer2.Stop();
	}
}
